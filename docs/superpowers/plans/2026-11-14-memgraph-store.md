# Memgraph Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the last id hole (module-level names equal to a submodule), then store each project's code graph in Memgraph from `workbench index`, incrementally and per namespace.

**Architecture:**
- **Id fix:** the parser learns the project's module names through `SourceFile.modules`, and suffixes any module-level name that equals one of them.
- **Storage module:** `storage.rs` becomes a module. A `memgraph` submodule, built by default, holds pure functions that turn `FileGraph`s into `UNWIND` batches, and a `MemgraphStore` that implements `Storage` over `rsmgclient`, with one transaction per project.
- **`index`:** connects before taking the lock, and cleans up projects removed from the config after full runs.

**Tech Stack:** Rust 2024, rsmgclient 4.0.0 (synchronous; `autocommit` defaults to off, `lazy` defaults to on), Memgraph 3.14.0.

**Spec:** `docs/superpowers/specs/2026-11-14-memgraph-store-design.md`

## Global Constraints

- **Node ids:** `<project-id>:<qualified name>`; suffix order within an id is qualified name, `#n`, then `@pyi` or `@<path>`.
- **Clippy** with `-D warnings`; `unwrap_used` and `panic` are denied, in tests too; `unsafe_code` is denied, so tests must not call `std::env::set_var`.
- **Tests** live in a separate `tests.rs` beside each module's `mod.rs`.
- **Integration tests** against Memgraph run only when `WORKBENCH_TEST_MEMGRAPH=host:port` is set; otherwise they return early (pass). Each one uses its own namespace and deletes it afterwards.
- **Memgraph integers** are signed 64-bit, so hashes are stored as 16 lowercase hex digits.
- **Index and constraint creation** must run with autocommit on; Memgraph rejects them inside a multi-statement transaction.
- **With `lazy` on,** every `execute` must be followed by `fetchall` (or a commit) before the next query.
- **Commit messages** carry no Claude attribution or any other trace of Claude.

## Review Focus

1. **A submodule added next to an unchanged `__init__.py` that defines that name** (`pkg/settings.py` added later): the next run must re-parse `pkg/__init__.py` and suffix the name, even though its contents didn't change. The fix is to fold the file's direct-child module names into `file_hash`. Tested in Task 1.
2. **Two projects with the same file path** (both have `main.py`): writing or deleting one project's file never touches the other's. Tested in Task 4.
3. **`storage.password_env` names an unset variable:** a friendly error before any parsing, and no panic. Tested in Task 2.
4. **An interrupted full run:** it must not delete any project's data. Tested in Task 5.
5. **A run with `--id`:** it must never remove other projects' data. Tested in Task 5.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/graph/mod.rs` (modify) | `SourceFile.modules` |
| `src/cli/commands/index/parser/python/mod.rs`, `tests.rs` (modify) | Suffix module-level names equal to a submodule |
| `src/cli/workspace.rs` → `src/cli/workspace/mod.rs` + `tests.rs` (move) | `StorageSettings.username` and `password_env`, and password lookup |
| `src/cli/errors.rs` (modify) | New errors for connection, namespace, password variable and cleanup |
| `src/cli/commands/index/storage.rs` → `storage/mod.rs` (move) | The `Storage` trait (new signatures), and the no-Memgraph placeholder |
| `src/cli/commands/index/storage/memgraph/mod.rs` (create) | `MemgraphStore` |
| `src/cli/commands/index/storage/memgraph/batches.rs` (create) | Graphs → `UNWIND` rows (pure) |
| `src/cli/commands/index/storage/memgraph/tests.rs` (create) | Batch unit tests, and opt-in integration tests |
| `src/cli/commands/index/mod.rs`, `report.rs`, `tests.rs` (modify) | Module names, connecting, cleanup, summary line |
| `Cargo.toml`, `README.md` (modify) | Default feature; docs |

---

### Task 1: Module-level names equal to a submodule

**Files:** `src/graph/mod.rs`, `src/graph/tests.rs`, `parser/python/mod.rs`, `parser/python/tests.rs`, `src/cli/commands/index/mod.rs`, `src/cli/commands/index/tests.rs`

**Interfaces:**
- **In `graph`:** `SourceFile` gains `pub modules: Option<&'a HashSet<String>>`, the module names of every file in the project.
- **In the parser:** `Walk` gains `modules: Option<&'a HashSet<String>>` and `path: &'a str`. A new method `fn claim(&self, qualified_name: String) -> String` returns `qualified_name + "@" + path` when the walk is in module scope (`frames.len() == 1`), `modules` contains `qualified_name`, and `qualified_name != self.module.name`. Otherwise it returns `qualified_name` unchanged.
  - `define` uses `self.claim(scope.qualify(name))`.
  - Variables use `self.claim(join(module, name))`, both for their binding and for their node id.
- **In `index`:**
  - `collision_losers` becomes `fn project_modules(naming: &ModuleNaming<'_>, all_files: &[PathBuf], logger: &Logger) -> ProjectModules`, where `struct ProjectModules { losers: HashSet<String>, names: HashSet<String> }`.
  - `ProjectFiles` gains `modules: &'a HashSet<String>`; `index_file` passes `Some(files.modules)`.
  - `file_hash` also folds in, sorted and joined by `\0`, the module names that are direct children of the file's module: names `M` with `M == format!("{own}.{last}")` and no further dot after `own.`.

- [ ] **Step 1: Write the failing tests.** Add `modules: None` to every existing `SourceFile` literal.

```rust
// python/tests.rs
#[test]
fn module_level_names_equal_to_a_submodule_get_the_path_suffix() {
    let modules: HashSet<String> = ["pkg", "pkg.settings", "pkg.Loader"].map(str::to_owned).into();
    let file = SourceFile { project: "api", path: "pkg/__init__.py", src_is_package: false, disambiguator: None, modules: Some(&modules) };
    let source = "settings = load()\nclass Loader:\n    def run(self): ...\nclass Other(Loader): ...\nother = 1\n";
    let graph = parse_file(&file, source).graph;
    let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids, [
        "api:pkg",
        "api:pkg.Loader@pkg/__init__.py",
        "api:pkg.Loader@pkg/__init__.py.run",
        "api:pkg.Other",
        "api:pkg.settings@pkg/__init__.py",
        "api:pkg.other",
    ]);
    assert_eq!(inherits_targets(&graph), ["pkg.Loader@pkg/__init__.py"]);
}
```

```rust
// index/tests.rs
#[test]
fn adding_a_submodule_reparses_the_init_that_defines_its_name() -> CliResult<()> {
    let scratch = Scratch::new("submodule", TWO_PROJECTS)?;
    scratch.write("api/pkg/__init__.py", "settings = 1\n")?;
    scratch.write("api/pkg/settings.py", "")?;
    // The last run saw only `pkg/__init__.py`, with this content and no `pkg.settings` module.
    let storage = FakeStorage {
        stored: HashMap::from([("api".to_owned(), HashMap::from([("pkg/__init__.py".to_owned(), content_hash(b"settings = 1\n"))]))]),
        ..FakeStorage::default()
    };
    scratch.run(&only("api"), &Run { storage: &storage, python: &PythonParser, typescript: &TextParser, interrupted: false })?;
    let written = storage.written.borrow();
    let init = written[0].1.upserted.iter().find(|graph| graph.path == "pkg/__init__.py");
    assert!(init.is_some_and(|graph| graph.nodes.iter().any(|node| node.id == "api:pkg.settings@pkg/__init__.py")));
    Ok(())
}
```

- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test`. Expected: a compile error, since `modules` does not exist.
- [ ] **Step 3: Implement it:** `SourceFile.modules`, `Walk::claim`, `project_modules`, and the child-module fold in `file_hash`.
- [ ] **Step 4: Run everything.** Run `cargo test --locked && cargo clippy --workspace --all-targets --locked -- -D warnings`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit.** `git commit -am "Suffix module-level names that equal a submodule"`

---

### Task 2: Storage settings and connection errors

**Files:**
- Move: `src/cli/workspace.rs` → `src/cli/workspace/mod.rs`
- Create: `src/cli/workspace/tests.rs`
- Modify: `src/cli/errors.rs`

**Interfaces:**
- **`StorageSettings`** gains `pub username: Option<String>` and `pub password_env: Option<String>`, both read from `storage.username` and `storage.password_env`.
- **`impl StorageSettings { pub fn password(&self, lookup: impl Fn(&str) -> Option<String>) -> CliResult<Option<String>> }`:**
  - `Ok(None)` without `password_env`;
  - `Ok(Some(value))` when `lookup` finds it;
  - otherwise `Err(CliErrors::MissingPasswordVariable(name))`.
  - Production passes `|name| std::env::var(name).ok()`.
- **New `CliErrors` variants,** with their `Display` text:
  - `MemgraphConnection { address: String, details: String }` → ``Could not connect to Memgraph at {address}. Is it running?`` (the details go to the log through `Debug`);
  - `MissingStorageNamespace(PathBuf)` → ``The workspace config {path} has no `storage.namespace`. Add one under `storage`.``;
  - `MissingPasswordVariable(String)` → ```storage.password_env` names the environment variable `{name}`, which is not set.``;
  - `ProjectCleanupFailed(String)` → `Could not remove the indexed data of projects no longer in the config.`

- [ ] **Step 1: Write the failing tests** in `workspace/tests.rs`. Write the config into a temporary workspace, as the other tests do, and use `Workspace::find`.

```rust
#[test]
fn storage_reads_username_and_password_variable() -> CliResult<()> {
    let workspace = scratch_workspace("storage-auth", "version: 1\nstorage:\n  host: db\n  port: 7688\n  namespace: ns\n  username: wb\n  password_env: MG_PASS\nprojects: []\n")?;
    let settings = workspace.storage()?;
    assert_eq!((settings.host.as_str(), settings.port, settings.namespace.as_deref()), ("db", 7688, Some("ns")));
    assert_eq!(settings.username.as_deref(), Some("wb"));
    assert_eq!(settings.password(|name| (name == "MG_PASS").then(|| "secret".to_owned()))?, Some("secret".to_owned()));
    assert!(matches!(settings.password(|_| None), Err(CliErrors::MissingPasswordVariable(name)) if name == "MG_PASS"));
    Ok(())
}

#[test]
fn storage_without_a_password_variable_has_no_password() -> CliResult<()> {
    let workspace = scratch_workspace("storage-plain", "version: 1\nprojects: []\n")?;
    let settings = workspace.storage()?;
    assert_eq!((settings.host.as_str(), settings.port, settings.namespace.as_deref()), ("localhost", 7687, None));
    assert_eq!(settings.password(|_| Some("ignored".to_owned()))?, None);
    Ok(())
}

#[test]
fn connection_errors_keep_details_out_of_the_message() {
    let error = CliErrors::MemgraphConnection { address: "localhost:7687".to_owned(), details: "Connection refused".to_owned() };
    assert_eq!(error.to_string(), "Could not connect to Memgraph at localhost:7687. Is it running?");
    assert!(format!("{error:?}").contains("Connection refused"));
}
```

  `scratch_workspace(name, config) -> CliResult<ScratchWorkspace>` is a test helper that removes its directory on drop, like the others.
- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test workspace::`. Expected: a compile error, since the fields and variants are missing.
- [ ] **Step 3: Implement the move** (`git mv src/cli/workspace.rs src/cli/workspace/mod.rs`), the new fields, `password`, and the error variants.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test --locked`. Expected: all pass.
- [ ] **Step 5: Commit.** `git add -A src && git commit -m "Read Memgraph credentials from the storage settings"`

---

### Task 3: Graphs to write batches

**Files:**
- Move: `src/cli/commands/index/storage.rs` → `src/cli/commands/index/storage/mod.rs`
- Create: `storage/memgraph/mod.rs` (only `mod batches;` and `#[cfg(test)] mod tests;` for now), `storage/memgraph/batches.rs`, `storage/memgraph/tests.rs`
- Modify: `Cargo.toml`: `[features] default = ["memgraph"]`. `storage/mod.rs` gets `#[cfg(feature = "memgraph")] pub mod memgraph;`.

**Interfaces** (in `batches.rs`, all `pub(super)`):
- `fn hex(hash: u64) -> String` returns `format!("{hash:016x}")`.
- `struct Batches { files: Vec<QueryParam>, nodes: Vec<(&'static str, Vec<QueryParam>)>, contains: Vec<QueryParam>, references: Vec<(&'static str, Vec<QueryParam>)> }`.
- `fn batches(graphs: &[FileGraph]) -> Batches`. Each row is a `QueryParam::Map`:
  - **`files`:** `{path, hash: hex, module}`.
  - **`nodes`:** grouped by label (`"Module"`, `"Class"`, `"Function"`, `"Method"`, `"Variable"`), with only non-empty groups, in that order. Each row is `{id, kind: label, name, path: file path, start_line, end_line}`, with lines as `Int`.
  - **`contains`:** `{from, to}`, from every graph's edges.
  - **`references`:** grouped by relationship type (`"IMPORTS"`, `"INHERITS"`), non-empty groups only. Each row is `{from, target, line, path}`.
- **Labels and relationship types** are static strings because Cypher cannot take them as parameters; the store writes one query per group.

- [ ] **Step 1: Write the failing tests** in `memgraph/tests.rs`. Use a helper `text(row: &QueryParam, key: &str) -> Option<&str>`, which reads a `QueryParam::String` from a `QueryParam::Map`.

```rust
#[test]
fn hashes_are_sixteen_hex_digits() {
    assert_eq!(hex(0), "0000000000000000");
    assert_eq!(hex(u64::MAX), "ffffffffffffffff");
}

#[test]
fn batches_group_nodes_by_label_and_references_by_type() {
    let graph = parse(&plain("api", "billing/models.py"), "from .base import Model\nclass Invoice(Model):\n    def total(self): ...\nrate = 1\n");
    let batches = batches(&[graph]);
    assert_eq!(batches.files.len(), 1);
    assert_eq!(text(&batches.files[0], "path"), Some("billing/models.py"));
    assert_eq!(text(&batches.files[0], "module"), Some("api:billing.models"));
    let labels: Vec<(&str, usize)> = batches.nodes.iter().map(|(label, rows)| (*label, rows.len())).collect();
    assert_eq!(labels, [("Module", 1), ("Class", 1), ("Method", 1), ("Variable", 1)]);
    assert_eq!(text(&batches.nodes[2].1[0], "id"), Some("api:billing.models.Invoice.total"));
    assert_eq!(text(&batches.nodes[2].1[0], "path"), Some("billing/models.py"));
    assert_eq!(batches.contains.len(), 3);
    let types: Vec<&str> = batches.references.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(types, ["IMPORTS", "INHERITS"]);
}
```

  `parse` is a test helper that calls `PythonParser.parse(..)` and returns the graph of a `Parsed` outcome, or `Err` otherwise; the test returns `Result`. `plain` builds a `SourceFile` with all options off.
- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test memgraph::`. Expected: a compile error, since `batches` and `hex` are missing.
- [ ] **Step 3: Implement the move, the default feature, and `batches.rs`.**
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test --locked`. Expected: all pass. The placeholder is still in use, and the build needs cmake and OpenSSL 3 from now on.
- [ ] **Step 5: Commit.** `git add -A Cargo.toml Cargo.lock src && git commit -m "Turn code graphs into Memgraph write batches"`

---

### Task 4: MemgraphStore

**Files:** `storage/mod.rs`, `storage/memgraph/mod.rs`, `storage/memgraph/tests.rs`, `src/cli/commands/index/mod.rs`, `src/cli/commands/index/tests.rs`

**Interfaces:**
- **`Storage` trait,** in `storage/mod.rs`:
  - `fn write_project(&self, project: &ExistingProject, changes: &ProjectChanges) -> Result<WriteResult, StorageError>` replaces the `project: &str` parameter.
  - New: `fn remove_projects_except(&self, keep: &[&str]) -> Result<Vec<String>, StorageError>`, which returns the removed project ids, sorted.
  - The placeholder returns `Ok(Vec::new())`. `index_project` passes `project`. `FakeStorage` in `index/tests.rs` implements the new method as `Ok(Vec::new())`; Task 5 adds its recording fields.
- **`MemgraphStore`,** in `memgraph/mod.rs`:
  - `pub struct MemgraphStore { connection: RefCell<Connection>, namespace: String, address: String }`.
  - `pub fn connect(settings: &StorageSettings, namespace: &str, password: Option<String>) -> Result<Self, StorageError>`:
    - `ConnectParams { host, port, username, password, lazy: false, ..Default::default() }`;
    - then, with autocommit on, it creates the indexes `Code(id)`, `File(path)`, `Project(id)` and `Workspace(namespace)`, and the unique constraint `ON (c:Code) ASSERT c.namespace, c.id IS UNIQUE`;
    - "already exists" outcomes must not fail;
    - then autocommit is turned off.
  - `impl Storage for MemgraphStore`:
    - **`stored_hashes`:** returns hex hashes parsed back with `u64::from_str_radix(.., 16)`. Unparseable hashes are skipped, so that file is simply re-parsed.
    - **`write_project`:** the transaction in spec §3; any error rolls back.
    - **`remove_projects_except`:** one transaction.
    - **`describe`:** `"Storage: Memgraph at {address}, namespace {namespace}"`.
  - **Test-only:** `#[cfg(test)] fn delete_namespace(&self) -> Result<(), StorageError>` detaches and deletes every node with this namespace.
- **Errors:** `MgError` maps to `StorageError(error.to_string())`.

- [ ] **Step 1: Write the failing integration tests** in `memgraph/tests.rs`.
  - **Helper:** `fn store(name: &str) -> Result<Option<Store>, StorageError>`. It reads `WORKBENCH_TEST_MEMGRAPH`, and returns `Ok(None)` when the variable is unset; each test then returns `Ok(())` early. It connects with namespace `wb-test-{name}-{pid}`. `Store` wraps `MemgraphStore` and calls `delete_namespace` on drop.
  - Each test builds `ProjectChanges` from real parses.

```rust
#[test]
fn written_files_read_back_as_hashes() -> TestResult { /* write api with a.py, b.py; stored_hashes("api") == {a.py: h_a, b.py: h_b} */ }

#[test]
fn changed_and_removed_files_replace_their_nodes() -> TestResult { /* write a.py v1 (class Old), b.py; then upsert a.py v2 (class New) + remove b.py;
    query (store test helper `ids(project)`) == [api:a, api:a.New]; stored_hashes has only a.py with h_v2 */ }

#[test]
fn a_failed_write_rolls_back() -> TestResult { /* write a.py; then a write whose two graphs share a node id (unique constraint) -> Err;
    stored_hashes still == {a.py: h_a} and ids unchanged */ }

#[test]
fn projects_with_the_same_file_path_stay_apart() -> TestResult { /* write api/main.py and web/main.py; remove main.py from api;
    stored_hashes("web") still has main.py */ }

#[test]
fn namespaces_do_not_see_each_other() -> TestResult { /* two stores (two names), same project/file; each sees only its own hash */ }

#[test]
fn removed_projects_are_deleted() -> TestResult { /* write api and old; remove_projects_except(&["api"]) == ["old"]; stored_hashes("old") empty, api intact */ }
```

  Add a test-only `fn ids(&self, project: &str) -> Result<Vec<String>, StorageError>` (code node ids, sorted) to `MemgraphStore` for these assertions. `TestResult` is `Result<(), Box<dyn std::error::Error>>`.
- [ ] **Step 2: Run the tests and see them fail.** Run `WORKBENCH_TEST_MEMGRAPH=127.0.0.1:7687 cargo test memgraph::`. Expected: a compile error, since `MemgraphStore` doesn't exist. Start the container first if it is stopped: `container start memgraph`.
- [ ] **Step 3: Implement the trait change and `MemgraphStore`.**
- [ ] **Step 4: Run the tests and see them pass.**
  - Run `WORKBENCH_TEST_MEMGRAPH=127.0.0.1:7687 cargo test --locked`. Expected: all pass, including the 6 integration tests.
  - Then run `cargo test --locked` without the variable. Expected: all pass, with the integration tests returning early.
- [ ] **Step 5: Commit.** `git add -A src && git commit -m "Store code graphs in Memgraph"`

---

### Task 5: Wire the store into `workbench index`

**Files:** `src/cli/commands/index/mod.rs`, `report.rs`, `tests.rs`, `README.md`, `docs/superpowers/plans/2026-11-14-python-ast-graph.md` (the memgraph next step)

**Interfaces:**
- **`run()`,** with the `memgraph` feature:
  1. Read `workspace.storage()?`.
  2. Require the namespace, else `CliErrors::MissingStorageNamespace(workspace.config_path())`.
  3. Get `password(|name| env::var(name).ok())?`.
  4. Call `MemgraphStore::connect(..)`. A failure becomes `CliErrors::MemgraphConnection { address: "{host}:{port}", details }`.

  All of this happens before `execute`, and therefore before the lock. Without the feature, `run` uses the placeholder, renamed `NoStorage`, whose storage line is `Storage: none (built without Memgraph support; nothing was written)`. It is compiled only `#[cfg(not(feature = "memgraph"))]`.
- **`IndexRun`** gains `removed_projects: Vec<String>` and `cleanup_failed: bool`.
- **Cleanup in `execute`:** after the project loop, when `args.id.is_none() && !run.interrupted`, call `storage.remove_projects_except(&config_ids)`. On success, fill `removed_projects`. On error, log the details and set `cleanup_failed`.
- **`IndexRun::problems`** adds `"the data of projects removed from the config could not be deleted"` when `cleanup_failed` is set.
- **`IndexRun::summary`** adds, after the table and before the storage line, `Removed indexed data of {n} project(s) no longer in the config: {ids joined ", "}` when there are any.

- [ ] **Step 1: Write the failing tests** in `index/tests.rs`.

```rust
#[test]
fn a_full_run_removes_projects_no_longer_in_the_config() -> CliResult<()> {
    let scratch = Scratch::new("cleanup", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;
    let storage = FakeStorage { removable: vec!["old-api".to_owned()], ..FakeStorage::default() };
    let run = scratch.index_with(&all(), &storage, &TextParser, false)?;
    assert_eq!(storage.kept.borrow().as_slice(), [vec!["api".to_owned(), "web".to_owned()]]);
    assert_eq!(run.removed_projects, ["old-api"]);
    assert!(run.summary("Storage: fake", false).contains(&"Removed indexed data of 1 project no longer in the config: old-api".to_owned()));
    Ok(())
}

#[test]
fn runs_with_an_id_or_interrupted_runs_remove_nothing() -> CliResult<()> {
    let scratch = Scratch::new("no-cleanup", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;
    let storage = FakeStorage { removable: vec!["old-api".to_owned()], ..FakeStorage::default() };
    scratch.index_with(&only("api"), &storage, &TextParser, false)?;
    scratch.index_with(&all(), &storage, &TextParser, true)?;
    assert!(storage.kept.borrow().is_empty());
    Ok(())
}

#[test]
fn a_failed_cleanup_is_a_problem() -> CliResult<()> {
    let scratch = Scratch::new("cleanup-fails", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;
    let storage = FakeStorage { fail_cleanup: true, ..FakeStorage::default() };
    let run = scratch.index_with(&all(), &storage, &TextParser, false)?;
    assert_eq!(run.problems(), ["the data of projects removed from the config could not be deleted"]);
    Ok(())
}
```

  `FakeStorage` gains `removable: Vec<String>`, `fail_cleanup: bool` and `kept: RefCell<Vec<Vec<String>>>`.
- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test index::`. Expected: compile errors, since the fields don't exist.
- [ ] **Step 3: Implement the cleanup, the summary line, the problem text, and `run()`'s connection.**
- [ ] **Step 4: Run everything.**
  - Run `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --locked`.
  - Also run `cargo build --no-default-features`.
  - Expected: all pass and both builds succeed.
- [ ] **Step 5: Update the docs.**
  - **README, `workbench index`:** replace the placeholder note with a description of what is stored (the graph in spec §2), the incremental behaviour, the cleanup, and the connection error. Under the `storage` config, document `username` and `password_env`.
  - **README, build section:** add that a default build needs cmake and OpenSSL 3 (`brew install cmake openssl@3` on macOS), and that `--no-default-features` builds without Memgraph.
  - **The first plan's "Next steps":** add a line saying the Memgraph writer is done, with a pointer to this plan.
- [ ] **Step 6: Run against Memgraph for real,** on a fresh copy of `test-project`, with the container running.
  1. Run `index --id demo` twice. Expected: the first run has 36 parsed; the second has 36 unchanged and exit 0.
  2. Delete a file from the copy and run again. Expected: 1 removed.
  3. Query Memgraph with a scratch `rsmgclient` program in the scratchpad:
     `MATCH (c:Class {namespace: $ns})-[:INHERITS]->(r:Reference {target: 'src.models.main_model.MainModel'}) RETURN c.id`
     Expected: the 7 model classes.
  4. Delete the copy's namespace afterwards.
- [ ] **Step 7: Commit.** `git add -A src README.md docs && git commit -m "Write the index to Memgraph and remove deleted projects' data"`
