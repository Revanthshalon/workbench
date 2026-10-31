# Low-level design: workspace model and CLI Slice A

Status: **draft for review.** Implements [`SPEC-workspace.md`](../spec/SPEC-workspace.md) and Slice A of [`SPEC-cli.md`](../spec/SPEC-cli.md) (`init`, `project add | list | remove`, discovery, common behaviour). System context: [`HLD-workbench.md`](../hld/HLD-workbench.md). Tasks: [`tasks/todo.md`](../../tasks/todo.md). The specs win on behaviour; this document fixes types, signatures and mechanics. Choices the specs leave open are marked **LLD decision**.

## 1. Crates and files

```
lib/engine/src/
├── lib.rs  error.rs  ports.rs  init.rs  domain.rs (Platform, kept)
└── workspace/
    ├── mod.rs      re-exports and the use cases: list_projects, add_project, remove_project
    ├── project.rs  ProjectId, Language, DependencyManager, Project, Environment
    ├── lsp.rs      LspSettings, Value, effective settings
    ├── config.rs   WorkspaceConfig, StorageSettings, Namespace, EnvVarName, ComposeFile, SemanticLlm
    ├── scope.rs    ProjectScope
    ├── paths.rs    relative_to (pure)
    ├── add.rs  remove.rs
    └── memory.rs   templates and seed builder
lib/adapters/src/
├── lib.rs  dir_file.rs  workspace_file.rs  memory_dir.rs
├── gitignore_file.rs  project_inspector.rs  id_source.rs
services/cli/src/
├── main.rs  cli.rs  output.rs  utils/
services/cli/tests/cli.rs
```

Dependencies: `engine` → `thiserror`. `adapters` → `engine`, `serde`, `yaml_serde`. `workbench` → `engine`, `adapters`, `clap`. No new crates.

## 2. Engine types

```rust
/// A project's stable id: `[a-z][a-z0-9-]{0,63}`.
pub struct ProjectId(String);
impl FromStr for ProjectId { /* spec snippet */ }
impl ProjectId {
    /// Derives an id from a directory name; `Err` when nothing valid results.
    pub fn derive(dir_name: &str) -> Result<Self, Error>;
}

pub enum Language { Python, Angular }
pub enum DependencyManager { Poetry, Uv, Npm, Pnpm, Yarn, Bun }
impl DependencyManager {
    pub fn supports(self, language: Language) -> bool;   // poetry|uv: Python; others: Angular
}

pub struct Project {
    pub id: ProjectId,
    pub path: PathBuf,                    // as stored: relative to the root, or absolute
    pub language: Language,
    pub dependency_manager: DependencyManager,
    pub compose: Option<PathBuf>,
    pub environment: Option<Environment>,
    pub lsp: Option<LspSettings>,
}
pub struct Environment { pub venv: Option<PathBuf>, pub activate: Option<String> }

pub struct WorkspaceConfig {            // fields private; built by `WorkspaceConfig::new(parts)?`
    lsp: BTreeMap<Language, LspSettings>,
    storage: StorageSettings,
    compose: Vec<ComposeFile>,          // path, services: BTreeMap<String, ProjectId>
    semantic: Option<SemanticLlm>,
    projects: Vec<Project>,
}
impl WorkspaceConfig {
    pub fn projects(&self) -> &[Project];
    pub fn project(&self, id: &ProjectId) -> Option<&Project>;
    pub fn with_project(self, p: Project) -> Result<Self, Error>;   // re-validates
    pub fn without_project(self, id: &ProjectId) -> Result<Self, Error>;
    pub fn effective_lsp(&self, id: &ProjectId) -> Option<LspSettings>;
}
```

`new` is the single validation point (spec "Project rules"): version is checked by the adapter (it is not part of the model), the rest by `new`. A removed project that a `compose[].services` entry still names makes `without_project` fail with a hint to edit the config (**LLD decision**: silently dropping a person's mapping is worse).

**Storage keeps absence.** The spec says defaults are not written back, so the model stores what was written:

```rust
pub struct StorageSettings {
    host: Option<String>, port: Option<u16>, username: Option<String>,
    password_env: Option<EnvVarName>, namespace: Option<Namespace>,
}
impl StorageSettings {
    pub fn host(&self) -> &str { self.host.as_deref().unwrap_or("localhost") }
    pub fn port(&self) -> u16 { self.port.unwrap_or(7687) }
    pub fn namespace(&self) -> &str { /* "default" when absent */ }
    pub fn written(host, port, namespace) -> Self;   // what `init` writes: all three set
}
```

`Namespace` (`[a-z0-9][a-z0-9-]{0,63}`) and `EnvVarName` (`[A-Za-z_][A-Za-z0-9_]*`) are parse-don't-validate newtypes.

**LSP.** `LspSettings { enabled: Option<bool>, server: Option<String>, command: Option<Vec<String>>, settings: Option<BTreeMap<String, Value>> }`. Merge is per field, project over workspace default; `settings` is replaced as a whole, not deep-merged (**LLD decision**: the spec says "passed unchanged", so a partial map merge would invent a value). The catalog default is filled in later by the `lsp` module, so absent fields stay `None` here. `Value` is a small enum (`Null, Bool, Int, Float, Str, Seq, Map`) because the engine has no `serde`; the adapter converts from `yaml_serde::Value`.

**Scope.** `enum ProjectScope { All, Ids(Vec<ProjectId>) }`; `resolve(&self, &WorkspaceConfig) -> Result<Vec<&Project>, Error>` fails with `UnknownProject` for the first unknown id and drops duplicates.

## 3. Errors

One enum in `error.rs`. Each `Display` is `<what happened>. <what to do>`; `main` prints `Error: {e}` to stderr, so the common-behaviour format holds without extra code.

| Variant | Used for |
|---|---|
| `AlreadyExists(PathBuf)` (kept) | `init` when `config.yml` exists |
| `Storage(String)` (kept) | I/O failure in an adapter |
| `WorkspaceNotFound { from: PathBuf }` | `no .workbench/ found from <cwd>. Run 'workbench init'.` (exact text from the spec) |
| `InvalidProjectId(String)` | bad id or no valid derived id, hint names `--id` |
| `Invalid { project: Option<ProjectId>, field: &'static str, reason: String }` | every rule in the "Project rules" table; names project and field |
| `UnknownProject { id: String, known: Vec<String> }` | `remove`, scope resolution; lists known ids |
| `Undetected { what: &'static str, flag: &'static str }` | ambiguous or undetectable language/manager; names the flag |
| `NotADirectory(PathBuf)` | `project add` path, and later commands that read a project |

## 4. Ports (`ports.rs`)

```rust
pub trait WorkspaceStore {
    fn load(&self) -> Result<WorkspaceConfig, Error>;
    fn save(&self, config: &WorkspaceConfig) -> Result<(), Error>;   // atomic, see §6
    fn create_empty(&self, config: &WorkspaceConfig) -> Result<PathBuf, Error>; // AlreadyExists if present
}
pub trait MemoryStore {
    fn create(&self, id: &ProjectId, seed: &str) -> Result<Created, Error>;
    fn create_workspace(&self) -> Result<Created, Error>;
    fn discard(&self, created: &Created) -> Result<(), Error>;
    fn remove(&self, id: &ProjectId) -> Result<Vec<PathBuf>, Error>; // returns what it deleted
    fn list_files(&self, id: &ProjectId) -> Result<Vec<PathBuf>, Error>; // for the --purge-memory preview
}
pub struct Created(Vec<PathBuf>);             // only files/folders this call made
pub trait ContextSource { fn read(&self, project: &Path) -> Result<Vec<SourceFile>, Error>; }
pub trait ProjectInspector { fn inspect(&self, project: &Path) -> Result<Inspection, Error>; }
pub trait GitignoreStore { fn ensure_ignored(&self, root: &Path, entry: &str) -> Result<bool, Error>; }
pub trait IdSource { fn suffix(&self) -> String; }   // four lowercase hex chars
```

`create_empty` takes the config so `init` decides the content (namespace, `storage` block) and the adapter only writes it. `list_files` is an addition to the spec's port table, needed to print what `--purge-memory` will delete (**LLD decision**). Ports take the workspace root at construction (`WorkspaceFile::new(root)`), as `DirFileStore::new(dir)` does today.

`Inspection { language: Vec<Language>, python_managers: Vec<DependencyManager>, node_managers: Vec<DependencyManager>, venv: Option<PathBuf>, compose: Option<PathBuf> }` keeps *what was seen*; the use case decides ambiguity, so the rule lives in the engine.

## 5. Use cases

### `init_workspace`

```
init_workspace(workspace, memory, gitignore, ids, root, folder_name)
  1. namespace = sanitize(folder_name) + "-" + ids.suffix()      // see §7
  2. config = WorkspaceConfig::empty(StorageSettings::written("localhost", 7687, namespace))
  3. path = workspace.create_empty(&config)?      // AlreadyExists → return, nothing else touched
  4. memory.create_workspace()?                   // INIT-MEMORY.md
  5. gitignore.ensure_ignored(root, ".workbench/")?
  return InitReport { config_path, namespace, memory_created, gitignore_changed }
```

Step 3 is first so a repeat run changes nothing. If 4 or 5 fail after 3, the config exists and the person re-runs after deleting `.workbench/` (**LLD decision**: `init` is cheap to redo; rolling back would delete a directory we only partly created).

### `add_project`

```
add_project(ws, memory, ctx, inspector, req)      req: path(abs, resolved), root, id?, language?, manager?, venv?, activate?, compose?
  1. require is_dir(req.path)                              → NotADirectory
  2. inspection = inspector.inspect(path)
  3. language, manager = given or decided from inspection  → Undetected{flag} when ambiguous/none
  4. id = req.id or ProjectId::derive(dir_name)            → InvalidProjectId
  5. defaults: venv (.venv if seen, Python), activate (uv run | poetry run | none), compose (given or seen)
  6. stored_path = relative_to(root, path) or absolute
  7. config = ws.load()?.with_project(project)?            // validates, rejects duplicate id/path
  8. seed = memory::seed(ctx.read(path)?)
  9. created = memory.create(&id, &seed)?
 10. if let Err(e) = ws.save(&config) { memory.discard(&created); return Err(e) }
```

Order is memory then config, so the only state a failure can leave is memory files, and `discard(created)` removes exactly the ones this run made; files left by an earlier `project remove` are not in `created` and stay. `--compose` is checked to exist in step 5 (spec: error when given to `project add`).

### `list_projects` and `remove_project`

`list_projects(ws) -> Vec<ProjectRow>` (id, language, manager, stored path). `remove_project(ws, memory, id, purge)`: load, `without_project`, save; with `purge`, return the file list first (`list_files`) for the CLI to print, then `memory.remove`. Config is saved before the purge so a failed delete leaves a valid workspace. It never reads project paths, so it works when a directory is gone.

## 6. Adapters

- **`WorkspaceFile`** (`.workbench/config.yml`). DTOs mirror the YAML with `#[serde(deny_unknown_fields)]`; `Option` fields use `skip_serializing_if` so absent stays absent. `load` checks `version == 1`, converts DTO → engine types (parse-don't-validate constructors give the field-named errors), then calls `WorkspaceConfig::new`. A missing `path` surfaces as `Invalid { project, field: "path" }`, not a raw serde message (map the serde error text for `missing field` into it). `save` is **atomic** (**LLD decision**): write `config.yml.tmp` in the same directory, `fsync`, `rename` over the target, so a crash or full disk cannot leave a truncated config.
- **`MemoryDir`**. `create` uses `create_new` per file so an existing file is never overwritten, and records only paths it made (including the folder if it made it). `discard` deletes recorded files, then the folder only if it is now empty and was recorded.
- **`ContextSource`** (same struct). Reads `README.md`, `CLAUDE.md`, `AGENTS.md` at the project root if they exist and are regular files (symlinks followed, since the spec reads "files at the project root"; reading stays inside the project path); non-UTF-8 files are skipped; content is cut at 20 KB on a char boundary.
- **`ProjectInspector`**. Existence checks for the files in the detection table; for `pyproject.toml`, a line scan for `[tool.uv]`, `[tool.uv.`, `[tool.poetry]`, `[tool.poetry.` after `trim()`, marked `// ponytail: header-line match, not a TOML parse; a header inside a multi-line string is a false positive`. Runs nothing and imports nothing from the project.
- **`GitignoreStore`**. Reads the file; skips the append if a line equals `.workbench/` or `.workbench` after `trim()`; otherwise appends (adding a newline first when the file lacks one); creates the file if missing. Returns whether it changed anything.
- **`IdSource`**. Four hex digits from `std::hash::RandomState::new().build_hasher().finish()`, which is seeded from the OS.

## 7. Algorithms

**Id derivation** (`ProjectId::derive`): lowercase; map every char not `[a-z0-9]` to `-`; collapse runs; trim `-`; cut to 64; require it to start with `[a-z]`. `Billing_API` → `billing-api`; `3d-viewer`, `___` → `InvalidProjectId` with the `--id` hint.

**Namespace** (`init`): same sanitising but keep leading digits, cut to 59, fall back to `workspace` when empty, then `-` + 4 hex = at most 64 chars.

**`relative_to(root, path)`** (pure, both absolute and already `canonicalize`d by the CLI): split into components, drop the common prefix, emit one `..` per remaining root component, then the rest of `path`. Returns `None` when they share no prefix (different drives or roots), in which case the absolute path is stored. Table tests: under root, sibling (`../shared/web`), same dir, no common prefix.

**Workspace discovery** (`cli`): from the current directory (or `--workspace`), check `<dir>/.workbench/`, then the parent, up to the filesystem root. The workspace root is the directory that contains it. `--workspace` is used as the root directly and must contain `.workbench/`, else `WorkspaceNotFound`.

## 8. CLI

`cli.rs`: `Cli { workspace: Option<PathBuf>, command }`, `Command::{Init{out}, Project(ProjectCommand)}`, `ProjectCommand::{Add(AddArgs), List, Remove{id, purge_memory}}`. Enums via `LanguageArg` and `DependencyManagerArg` (the `PlatformArg` pattern) with `From` into the engine types. `--workspace` is `global = true`. `PlatformArg` is removed (nothing uses it in this slice). `--index` is not defined, so clap rejects it with exit 2.

`output.rs`: pure functions from reports to `String` (`init_report`, `project_rows`, `purge_preview`), so formatting is unit-tested without a process. `main.rs` stays wiring: parse, locate workspace, build adapters, call the use case, print, map `Err` to `Error: {e}` and exit 1. Progress/diagnostics to stderr, results to stdout. Row format for `project list` follows the spec snippet: tab-separated `id language manager path`; the empty hint goes to stdout.

## 9. Testing layout

| Level | Where | Covers |
|---|---|---|
| Engine unit | beside code, fake ports (`Fake*` structs in `#[cfg(test)]`) | every row of the Project rules table; id/namespace derivation; LSP merge; scope; `relative_to`; use-case rollback with a failure injected at each step |
| Adapter | `lib/adapters` `#[cfg(test)]` with unique dirs under `std::env::temp_dir()` and a drop guard | YAML round trip equals model; unknown key/version; `MemoryDir` never overwrites and `discard` removes only created; detection fixtures; `.gitignore` cases |
| Integration | `services/cli/tests/cli.rs` via `CARGO_BIN_EXE_workbench` | sequences `init → project add → list → remove`; exit codes (0, 1, 2); the exact `no .workbench/ found` text; subdirectory discovery; `--workspace` |

Fakes live in a `testing` module compiled only for tests; each fake records calls so rollback tests assert `discard` received exactly the created paths.

## 10. Sequence: `project add ../shared/web --id web`

```
CLI            engine::add_project      Inspector   ContextSource   MemoryDir     WorkspaceFile
 │ resolve+canonicalize path
 ├──────────────► is_dir? ─────────────►
 │                 inspect ────────────► (files only)
 │                 decide lang/manager (Undetected → exit 1, nothing written)
 │                 load ───────────────────────────────────────────────────────────► config
 │                 with_project (validate)
 │                 read seed ─────────────────────────► README/CLAUDE/AGENTS
 │                 create ───────────────────────────────────────────► INIT-MEMORY.md, annotations.yml
 │                 save ──────────────────────────────────────────────────────────► tmp + rename
 │                   └─ on failure: discard(created)                                  
 ◄── report (id, path, language, manager, files created)
```

## 11. Open points for review

1. **LLD decisions to confirm:** atomic save by temp file and rename; `list_files` added to `MemoryStore`; whole-map replace for LSP `settings`; `without_project` fails when a Compose `services` map names the project; no rollback of a half-done `init`.
2. The engine's own `Value` type for LSP settings (alternative: keep the raw YAML text as a string and parse later in the `lsp` module).
3. Symlinked context files are followed; say if they should be skipped.
