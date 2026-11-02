# Tasks: Slice A

Plan: [`plan.md`](plan.md). Specs: `SPEC-workspace.md`, Slice A of `SPEC-cli.md`.
Standing verification for every task: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Style: doc comments on public items, no `unwrap`/`panic`, parse-don't-validate. Test first.

## Phase 1: Foundation

- [ ] **T1: Workspace scaffold** (S)
  - Acceptance: root `Cargo.toml` lists members `lib/*` and `services/cli` and workspace dependency `adapters`; crates `engine` (`lib/engine`, depends on `thiserror`), `adapters` (`lib/adapters`, depends on `engine`, `serde`, `yaml_serde`) and `workbench` (`services/cli`, depends on `engine`, `adapters`, `clap`), each inheriting the workspace lints; engine has `Error` (`AlreadyExists`, `Storage`) and empty `workspace` module; the binary parses `--help` and exits 0. The old scaffold is removed in this task: `lib/engine/src/{domain,init,ports}.rs` (`Platform`, `ConfigStore`, the old `init`), `services/cli/src/adapters/` (`DirFileStore`), and the old `Init { platform, out }` clap command with `PlatformArg`.
  - Verify: standing checks; `cargo run -p workbench -- --help` prints help.
  - Files: `Cargo.toml`, `lib/engine/{Cargo.toml,src/{lib,error}.rs}` (and the deletions above), `lib/adapters/{Cargo.toml,src/lib.rs}`, `services/cli/{Cargo.toml,src/main.rs}`.
  - Depends on: none.

- [ ] **T2: Project identifiers and enums** (S)
  - Acceptance: `ProjectId` (`FromStr`, the spec's rules) and `ProjectId::derive(dir_name)` (lowercase, other chars to `-`, collapse, trim, cut to 64, invalid if no leading letter); `Language` (`python`, `typescript`), `Framework` (`angular`), `FileKind` (the eight kinds of `SPEC-lsp.md`), `DependencyManager`, the valid language/manager pair check and the framework/language check (`angular` only with `typescript`); `BinaryName` (same grammar as `ProjectId`); new `Error` variants name the id/field.
  - Verify: unit tests: accepted/rejected ids; `Billing_API` gives `billing-api`; `3d-viewer` and `___` invalid; long name cut to 64; each language/manager pair; `angular` with `python` rejected; `BinaryName` accepts and rejects like an id.
  - Files: `lib/engine/src/workspace/{mod,project}.rs`, `error.rs`, `lib.rs`.
  - Depends on: none (T1 not required).

- [ ] **T3: Project, environment and LSP precedence** (M)
  - Acceptance: `Project` (id, path, language, optional framework, manager, optional `binaries` as `Binary { name, entry }`, optional compose, optional `Environment { venv, activate }` with `venv` rejected for `typescript`, optional `LspSettings`); `LspSettings` with field-by-field merge, also per `FileKind` through a `servers` map (project over workspace default over catalog; `settings` replaced as a whole), an engine `Value` enum for `settings`, `enabled: false` respected. (The `WorkspaceConfig::effective_lsp` lookup is in T4.)
  - Verify: unit tests: partial override keeps other fields; `enabled: false`; venv on a `typescript` project rejected.
  - Files: `lib/engine/src/workspace/{project,lsp,mod}.rs`.
  - Depends on: T2.

- [ ] **T4: Workspace config and validation** (M)
  - Acceptance: `WorkspaceConfig` (version, `lsp.<language>` for `python|typescript`, `Storage` with defaults and validation, top-level `compose`, `semantic.llm`, projects). `new`/`validate` rejects: version ≠ 1, duplicate ids, duplicate resolved paths (nesting allowed), duplicate top-level compose path, unknown project id in `compose[].services`, bad port, bad namespace, bad env var names, `semantic.llm` rule violations (provider, model, endpoint: rejected for `copilot-cli`, `api_key_env`), `framework` with a language other than `typescript`, a duplicate binary name within a project, an unknown `lsp.servers` key, a binary `entry` that is empty or absolute, `lsp.server` (non-empty, no whitespace) and `lsp.command` (non-empty, no empty element). `effective_lsp(&id)` returns the merged settings (project over workspace default). Errors name project and field. Structure only; no filesystem access.
  - Verify: unit tests, one per row of the spec's "Project rules" table; `effective_lsp` with and without a workspace default.
  - Files: `lib/engine/src/workspace/{config,mod}.rs`, `error.rs`.
  - Depends on: T3.

- [ ] **T5: Project scope** (XS)
  - Acceptance: `ProjectScope::{All, Ids}` resolves against a config; an unknown id fails naming it; duplicates in the list collapse.
  - Verify: unit tests: all, subset, unknown id.
  - Files: `lib/engine/src/workspace/{scope,mod}.rs`.
  - Depends on: T4.

- [ ] **T6: Memory layout and ports** (M)
  - Acceptance: project and workspace `INIT-MEMORY.md` templates (fixed headings); `SourceFile { name, content, truncated }`; seed builder (headings + "Existing context" quoting each source under its name, adding `[truncated at 20 KB]` after a source whose `truncated` is set); ports `WorkspaceStore` (`create_empty(&config)`), `MemoryStore` (`create`→`Created`, `create_workspace`, `discard`, `remove`, `list_files`), `ProjectContextSource`, `IdSource`, `GitignoreStore`, `ProjectInspector` (`inspect(project, compose)`) the `#[cfg(test)]` `testing` module with recording fakes for these ports.
  - Verify: unit tests: sources found, none found, truncated source gets the marker.
  - Files: `lib/engine/src/workspace/memory.rs`, `ports.rs`, `lib.rs`, `lib/engine/src/testing.rs`.
  - Depends on: T2, T4.

### Checkpoint A: foundation
- [ ] Standing checks pass; `init --out ./tmp` still works; review with human.

## Phase 2: Adapters

- [ ] **T7: YAML workspace store** (M)
  - Acceptance: `WorkspaceFile` implements `WorkspaceStore`: `load` (serde DTOs, `deny_unknown_fields`, maps to the engine model, validates), `save` (atomic: temp file, `fsync`, rename), `create_empty`; a config whose project path does not exist still loads; missing `path` fails with a clear error naming the project. `create_empty` creates `.workbench/` and writes the given config, failing with `AlreadyExists` if `config.yml` exists. Adds `tempfile` as a dev-dependency of `adapters` (pinned version; run `cargo deny check`); adapter tests use `tempfile::TempDir`.
  - Verify: adapter tests: round trip with a config holding `lsp`, `compose`, `storage`, `semantic.llm` and two projects gives an equal model; unknown key rejected; unknown version rejected; `create_empty` twice fails the second time.
  - Files: `lib/adapters/src/{workspace_file,lib}.rs`.
  - Depends on: T4, T6.

- [ ] **T8: Memory and context adapters** (S)
  - Acceptance: `MemoryDir` implements `MemoryStore` (creates folder, `INIT-MEMORY.md`, empty `annotations.yml`; never overwrites; returns what it created; `discard` removes exactly those; `remove` deletes the folder) and `ProjectContextSource` (reads `README.md`, `CLAUDE.md`, `AGENTS.md` that exist, skips non-UTF-8, cuts at 20 KB on a char boundary and sets `SourceFile::truncated`).
  - Verify: adapter tests: existing copies of both files untouched; `discard` only removes created paths; `remove` deletes folder; an oversized file comes back cut with `truncated` set.
  - Files: `lib/adapters/src/{memory_dir,lib}.rs`.
  - Depends on: T6.

- [ ] **T9: Gitignore and id adapters** (XS)
  - Acceptance: `GitignoreFile::ensure_ignored(root, ".workbench/")` appends once; creates the file if missing; handles a file without a trailing newline. `RandomIds` implements `IdSource` from std only (four lowercase hex chars from `RandomState`).
  - Verify: adapter tests for the three gitignore cases plus "existing entry, nothing added"; two `RandomIds` calls return 4-char hex strings.
  - Files: `lib/adapters/src/{gitignore_file,id_source,lib}.rs`.
  - Depends on: T6.

- [ ] **T10: Project inspector adapter** (M)
  - Acceptance: `inspect(path, compose)` fails with `NotADirectory` when `path` is not a directory and with `Invalid { field: "compose" }` when a given `compose` (relative to the project) does not exist; otherwise per the detection table (angular.json → `typescript` + `angular`, pyproject.toml, `uv.lock`/`[tool.uv]`, `poetry.lock`/`[tool.poetry]`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `bun.lock`, `bun.lockb`) and `.venv`; returns what is detectable (language, framework, manager; language/manager may be absent or ambiguous); reads files only, runs nothing. `ponytail:` comment on the header-line match for `[tool.*]`.
  - Verify: fixture-directory tests: uv, poetry, `typescript` + `angular` with each lock file, both languages, both Python managers, empty; file instead of directory; missing and present `compose`.
  - Files: `lib/adapters/src/{project_inspector,lib}.rs`.
  - Depends on: T2, T6.

### Checkpoint B: adapters
- [ ] Standing checks pass; round trip equals model; review with human.

## Phase 3: Commands

- [ ] **T11: `init` use case** (S)
  - Acceptance: `init_workspace(workspace, memory, gitignore, ids, root, folder_name)` creates the empty config with `version: 1`, storage `localhost`/`7687`/namespace (sanitized folder name + `-` + `IdSource` suffix, ≤64 chars, `workspace` if empty), the workspace `INIT-MEMORY.md`, and the `.gitignore` entry; refuses if the config exists and changes nothing.
  - Verify: unit tests with fakes: refused when config exists (nothing written), namespace content, two suffixes give two namespaces, folder name with spaces/capitals/Unicode gives a valid namespace.
  - Files: `lib/engine/src/init.rs`, `workspace/config.rs`, `lib.rs`.
  - Depends on: T4, T6.

- [ ] **T12: `workbench init` end to end** (M)
  - Acceptance: CLI `init [--out]` (no `--platform`); prints what was created; `output.rs` with the `Error: <what>. <what to do>` formatter; exit codes 0/1 (clap gives 2); the integration-test harness in `tests/cli.rs`, using `tempfile::TempDir` (dev-dependency of `workbench`).
  - Verify: integration test: `init` in a temp dir creates `config.yml`, `memory/INIT-MEMORY.md` and the `.gitignore` line; a second run exits 1 with no changes; `--platform` exits 2; the written config loads back as an empty workspace.
  - Files: `services/cli/src/{cli,main,output}.rs`, `services/cli/tests/cli.rs`.
  - Depends on: T1, T7, T8, T9, T11.

- [ ] **T13: Workspace discovery and `project list`** (M)
  - Acceptance: `locate_workspace` searches upward for `.workbench/`; `--workspace <DIR>` overrides; not found prints `Error: no .workbench/ found from <cwd>. Run 'workbench init'.`; `list_projects` use case; `project list` prints `id  language  manager  path` per project (language written `typescript/angular` when a framework is set) and a hint when empty.
  - Verify: integration tests: works from a subdirectory; `--workspace` works; outside a workspace exits 1 with the exact message; empty list prints the hint.
  - Files: `services/cli/src/{cli,main,output}.rs`, `services/cli/src/utils/workspace.rs` (`locate_workspace`), `lib/engine/src/workspace/mod.rs` (use case), `services/cli/tests/cli.rs`.
  - Depends on: T7, T12.

- [ ] **T14: `add_project` use case** (M)
  - Acceptance: `add_project(request)` takes the already-resolved absolute path; the directory and a given `--compose` file are checked through `ProjectInspector::inspect(path, compose)` (the engine does no I/O), which fails with `NotADirectory` or a `compose` error; uses given flags or the inspection (language, framework, manager), derives the id (or fails naming `--id`), applies defaults (`--venv`, `--activate`, `--compose`), stores the path relative to the workspace root, builds and validates against the config, then `MemoryStore::create` (seeded from `ProjectContextSource`), then `save`. Any failure: config unchanged, `discard(created)` called, pre-existing files untouched. Ambiguous or undetectable fails naming the flag.
  - Verify: unit tests with fakes: not-a-directory and missing `--compose` fail before anything is written; failure injected at each step leaves config and memory unchanged; duplicate id rejected; duplicate resolved path rejected; re-add finds existing memory files and keeps them; relative-path table tests (`../x`, under root, absolute fallback).
  - Files: `lib/engine/src/workspace/{add,paths,mod}.rs`, `lib.rs`.
  - Depends on: T4, T6 (uses the inspector port; the adapter is T10).

- [ ] **T15: `workbench project add` end to end** (M)
  - Acceptance: `project add <PATH> [--id --language --framework --dependency-manager --venv --activate --compose]` (no `--index` in this slice; it is rejected as unknown with exit 2); enums via `LanguageArg`/`FrameworkArg`/`DependencyManagerArg`; no flag for binaries; prints what was created.
  - Verify: integration tests: uv, poetry and Angular (`typescript` + `angular`; npm, pnpm, yarn, bun) projects detect correctly; `--framework angular` with `--language python` exits 1 naming the field and create `INIT-MEMORY.md` plus an empty `annotations.yml`; ambiguous and undetectable fail naming the flag; failure leaves config and memory unchanged.
  - Files: `services/cli/src/{cli,main,output}.rs`, `services/cli/tests/cli.rs`, fixtures under `services/cli/tests/`.
  - Depends on: T13, T14, T8, T10.

- [ ] **T16: `workbench project remove` end to end** (S)
  - Acceptance: `remove_project(id, purge)` removes the entry and keeps `.workbench/memory/<id>/`; `--purge-memory` prints what it deletes then deletes the folder; unknown id fails listing known ids; works when the project directory no longer exists.
  - Verify: integration tests: remove keeps memory; purge deletes it; unknown id lists ids; remove after the directory was deleted succeeds; re-add finds both files.
  - Files: `lib/engine/src/workspace/{remove,mod}.rs`, `services/cli/src/{cli,main,output}.rs`, `services/cli/tests/cli.rs`.
  - Depends on: T13, T8.

### Checkpoint C: commands
- [ ] Spec success criteria 1–6 of `SPEC-cli.md` pass in the integration test; review with human.

## Phase 4: Finish

- [ ] **T17: Hardening and docs** (S)
  - Acceptance: every row of the detection table and every message in "Common behaviour" has a test; every row of the workspace "Project rules" table has a test, including `framework` and `binaries`; a config with `binaries` survives `project add` and `remove` unchanged; `--help` works on every command and the banner shows on top-level help only; `README.md`, `services/cli/README.md` and `lib/engine/README.md` updated (no `--platform` on `init`, new commands, `WorkspaceStore` instead of `ConfigStore`); empty `Makefile` left alone.
  - Verify: standing checks; `cargo run -p workbench -- --help` and `project add --help`.
  - Files: `services/cli/tests/cli.rs`, `README.md`, `services/cli/README.md`, `lib/engine/README.md`.
  - Depends on: T1–T16.

### Checkpoint D: done
- [ ] Workspace success criteria 1–7 and CLI Slice A criteria 1–6 and 15 met; ready for review.
