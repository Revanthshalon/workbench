# Tasks: Slice A

Plan: [`plan.md`](plan.md). Specs: `SPEC-workspace.md`, Slice A of `SPEC-cli.md`.
Standing verification for every task: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Style: doc comments on public items, no `unwrap`/`panic`, parse-don't-validate. Test first.

## Phase 1: Foundation

- [ ] **T1: Adapters crate, `DirFileStore` moved** (S)
  - Acceptance: new member `lib/adapters` (package `adapters`, depends on `engine`, `serde`, `yaml_serde`); `DirFileStore` and its `ConfigStore` impl live there; `services/cli` depends on it; behaviour unchanged.
  - Verify: standing checks; `cargo run -p workbench -- init --out ./tmp` still creates `./tmp/.workbench/config.yml` and a second run fails.
  - Files: `Cargo.toml`, `lib/adapters/Cargo.toml`, `lib/adapters/src/{lib,dir_file}.rs`, `services/cli/Cargo.toml`, `services/cli/src/{main,adapters/mod}.rs`.
  - Depends on: none.

- [ ] **T2: Project identifiers and enums** (S)
  - Acceptance: `ProjectId` (`FromStr`, the spec's rules) and `ProjectId::derive(dir_name)` (lowercase, other chars to `-`, collapse, trim, cut to 64, invalid if no leading letter); `Language`, `DependencyManager`, and the valid-pair check; new `Error` variants name the id/field.
  - Verify: unit tests: accepted/rejected ids; `Billing_API` gives `billing-api`; `3d-viewer` and `___` invalid; long name cut to 64; each language/manager pair.
  - Files: `lib/engine/src/workspace/{mod,project}.rs`, `error.rs`, `lib.rs`.
  - Depends on: none (T1 not required).

- [ ] **T3: Project, environment and LSP precedence** (M)
  - Acceptance: `Project` (id, path, language, manager, optional compose, optional `Environment { venv, activate }` with `venv` rejected for Angular, optional `LspSettings`); `LspSettings` with field-by-field merge (project over workspace default over catalog; `settings` replaced as a whole), an engine `Value` enum for `settings`, `enabled: false` respected.
  - Verify: unit tests: partial override keeps other fields; `enabled: false`; venv on Angular rejected.
  - Files: `lib/engine/src/workspace/{project,lsp,mod}.rs`.
  - Depends on: T2.

- [ ] **T4: Workspace config and validation** (M)
  - Acceptance: `WorkspaceConfig` (version, `lsp.<language>` for `python|angular`, `Storage` with defaults and validation, top-level `compose`, `semantic.llm`, projects). `new`/`validate` rejects: version ≠ 1, duplicate ids, duplicate resolved paths (nesting allowed), duplicate top-level compose path, unknown project id in `compose[].services`, bad port, bad namespace, bad env var names, `semantic.llm` rule violations (provider/model/api_key_env). Errors name project and field. Structure only; no filesystem access.
  - Verify: unit tests, one per row of the spec's "Project rules" table.
  - Files: `lib/engine/src/workspace/{config,mod}.rs`, `error.rs`.
  - Depends on: T3.

- [ ] **T5: Project scope** (XS)
  - Acceptance: `ProjectScope::{All, Ids}` resolves against a config; an unknown id fails naming it; duplicates in the list collapse.
  - Verify: unit tests: all, subset, unknown id.
  - Files: `lib/engine/src/workspace/{scope,mod}.rs`.
  - Depends on: T4.

- [ ] **T6: Memory layout and ports** (M)
  - Acceptance: project and workspace `INIT-MEMORY.md` templates (fixed headings); seed builder (headings + "Existing context" quoting each source under its name, 20 KB cap with `[truncated at 20 KB]`); ports `WorkspaceStore` (`create_empty(&config)`), `MemoryStore` (`create`→`Created`, `create_workspace`, `discard`, `remove`, `list_files`), `ContextSource`, `IdSource`, `GitignoreStore`, `ProjectInspector`; old `ConfigStore` removed.
  - Verify: unit tests: sources found, none found, oversized capped with marker.
  - Files: `lib/engine/src/workspace/memory.rs`, `ports.rs`, `lib.rs`, `init.rs` (compile fix only).
  - Depends on: T2.

### Checkpoint A: foundation
- [ ] Standing checks pass; `init --out ./tmp` still works; review with human.

## Phase 2: Adapters

- [ ] **T7: YAML workspace store** (M)
  - Acceptance: `WorkspaceFile` implements `WorkspaceStore`: `load` (serde DTOs, `deny_unknown_fields`, maps to the engine model, validates), `save` (atomic: temp file, `fsync`, rename), `create_empty`; a config whose project path does not exist still loads; missing `path` fails with a clear error naming the project.
  - Verify: adapter tests: round trip with a config holding `lsp`, `compose`, `storage`, `semantic.llm` and two projects gives an equal model; unknown key rejected; unknown version rejected.
  - Files: `lib/adapters/src/{workspace_file,lib}.rs`.
  - Depends on: T4.

- [ ] **T8: Memory and context adapters** (S)
  - Acceptance: `MemoryDir` implements `MemoryStore` (creates folder, `INIT-MEMORY.md`, empty `annotations.yml`; never overwrites; returns what it created; `discard` removes exactly those; `remove` deletes the folder) and `ContextSource` (reads `README.md`, `CLAUDE.md`, `AGENTS.md` that exist, skips non-UTF-8, caps at 20 KB).
  - Verify: adapter tests: existing copies of both files untouched; `discard` only removes created paths; `remove` deletes folder.
  - Files: `lib/adapters/src/{memory_dir,lib}.rs`.
  - Depends on: T6.

- [ ] **T9: Gitignore adapter** (XS)
  - Acceptance: `ensure_ignored(root, ".workbench/")` appends once; creates the file if missing; handles a file without a trailing newline.
  - Verify: adapter tests for the three cases plus "existing entry, nothing added".
  - Files: `lib/adapters/src/{gitignore_file,lib}.rs`.
  - Depends on: T1, T6.

- [ ] **T10: Project inspector adapter** (M)
  - Acceptance: `inspect(path)` per the detection table (angular.json, pyproject.toml, `uv.lock`/`[tool.uv]`, `poetry.lock`/`[tool.poetry]`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `bun.lock`, `bun.lockb`) and `.venv`; returns what is detectable (language/manager may be absent or ambiguous); reads files only, runs nothing. `ponytail:` comment on the header-line match for `[tool.*]`.
  - Verify: fixture-directory tests: uv, poetry, angular with each lock file, both languages, both Python managers, empty.
  - Files: `lib/adapters/src/{project_inspector,lib}.rs`.
  - Depends on: T2, T6.

### Checkpoint B: adapters
- [ ] Standing checks pass; round trip equals model; review with human.

## Phase 3: Commands

- [ ] **T11: `init` use case** (S)
  - Acceptance: `init_workspace(workspace, memory, gitignore, ids, folder_name)` creates the empty config with `version: 1`, storage `localhost`/`7687`/namespace (sanitized folder name + `-` + `IdSource` suffix, ≤64 chars, `workspace` if empty), the workspace `INIT-MEMORY.md`, and the `.gitignore` entry; refuses if the config exists and changes nothing.
  - Verify: unit tests with fakes: refused when config exists (nothing written), namespace content, two suffixes give two namespaces, folder name with spaces/capitals/Unicode gives a valid namespace.
  - Files: `lib/engine/src/init.rs`, `workspace/config.rs`, `lib.rs`.
  - Depends on: T4, T6.

- [ ] **T12: `workbench init` end to end** (M)
  - Acceptance: `IdSource` adapter (std only); CLI `init [--out]` without `--platform`; prints what was created; `PlatformArg` removed; `output.rs` created with the `Error: <what>. <what to do>` formatter; exit codes 0/1 (clap gives 2).
  - Verify: integration test: `init` in a temp dir creates `config.yml`, `memory/INIT-MEMORY.md` and the `.gitignore` line; a second run exits 1 with no changes; `--platform` now exits 2.
  - Files: `lib/adapters/src/{id_source,lib}.rs`, `services/cli/src/{cli,main,output}.rs`, `services/cli/tests/cli.rs`.
  - Depends on: T7, T8, T9, T11.

- [ ] **T13: Workspace discovery and `project list`** (M)
  - Acceptance: `locate_workspace` searches upward for `.workbench/`; `--workspace <DIR>` overrides; not found prints `Error: no .workbench/ found from <cwd>. Run 'workbench init'.`; `list_projects` use case; `project list` prints `id  language  manager  path` per project and a hint when empty.
  - Verify: integration tests: works from a subdirectory; `--workspace` works; outside a workspace exits 1 with the exact message; empty list prints the hint.
  - Files: `services/cli/src/{cli,main,output}.rs`, `lib/engine/src/workspace/mod.rs` (use case), `services/cli/tests/cli.rs`.
  - Depends on: T7, T12.

- [ ] **T14: `add_project` use case** (M)
  - Acceptance: `add_project(request)` resolves/validates the path, uses given flags or the inspector, derives the id (or fails naming `--id`), applies defaults (`--venv`, `--activate`, `--compose`), stores the path relative to the workspace root, builds and validates against the config, then `MemoryStore::create` (seeded from `ContextSource`), then `save`. Any failure: config unchanged, `discard(created)` called, pre-existing files untouched. Ambiguous or undetectable fails naming the flag.
  - Verify: unit tests with fakes: failure injected at each step leaves config and memory unchanged; duplicate id rejected; duplicate resolved path rejected; re-add finds existing memory files and keeps them; relative-path table tests (`../x`, under root, absolute fallback).
  - Files: `lib/engine/src/workspace/{add,paths,mod}.rs`, `lib.rs`.
  - Depends on: T4, T6, T10.

- [ ] **T15: `workbench project add` end to end** (M)
  - Acceptance: `project add <PATH> [--id --language --dependency-manager --venv --activate --compose]` (no `--index` in this slice; it is rejected as unknown with exit 2); enums via `LanguageArg`/`DependencyManagerArg`; prints what was created.
  - Verify: integration tests: uv, poetry and Angular (npm, pnpm, yarn, bun) projects detect correctly and create `INIT-MEMORY.md` plus an empty `annotations.yml`; ambiguous and undetectable fail naming the flag; failure leaves config and memory unchanged.
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
  - Acceptance: every row of the detection table and every message in "Common behaviour" has a test; every row of the workspace "Project rules" table has a test; `--help` works on every command and the banner shows on top-level help only; `README.md` and `services/cli/README.md` updated (no `--platform` on `init`, new commands); empty `Makefile` left alone.
  - Verify: standing checks; `cargo run -p workbench -- --help` and `project add --help`.
  - Files: `services/cli/tests/cli.rs`, `README.md`, `services/cli/README.md`.
  - Depends on: T1–T16.

### Checkpoint D: done
- [ ] Workspace success criteria 1–7 and CLI Slice A criteria 1–6 and 15 met; ready for review.
