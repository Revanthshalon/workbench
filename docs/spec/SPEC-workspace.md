# Spec: workspace

Module `workspace` from the [capability map](capability-map.md). Status: **approved.**

## Decisions

1. The workspace root is the directory that contains `.workbench/`. A relative project path is resolved against it.
2. A project does not have to sit under the root. The configured path is the source of truth: it can be relative to the root (`../other-repo`) or absolute, because a workspace can span several repos.
3. `language` is the project's main language: `python` or `angular`. YAML, Dockerfile and Docker Compose files are file types found *inside* projects, not project languages.
4. The new config replaces today's empty `config.yml`. `workbench init` now writes `version: 1`, a `storage` block (`host: localhost`, `port: 7687` and a unique `namespace`) and `projects: []`. Adding projects is a separate command, specified in `SPEC-cli.md`.
5. This module owns the *model, validation, storage ports and memory layout*. The commands that add projects live in `SPEC-cli.md`.
6. The file adapters (YAML config, memory folders, the existing `.workbench/` directory store) live in a new shared crate, `lib/adapters`, because both the CLI and the MCP server (`workbench mcp`) read the config.
7. The config carries `version: 1` so later changes can migrate old files.
8. *(Amendment)* The config can hold the settings for LLM-assisted semantic annotation, under `semantic.llm`. The supported providers are `copilot-cli`, `ollama` and `claude`. The settings are not secrets: provider, model, optional endpoint and, for `claude`, the *name* of the environment variable that holds the API key. The key itself is never stored in the config, not even inline.
9. *(Amendment, from `SPEC-core.md`)* The config also holds the storage settings, under `storage`, because `developer-support` and the CLI need storage without the server config. Workbench has one storage, Memgraph, running on the person's own machine. Its password is referenced by environment variable name, never stored. The meaning of each setting is defined in `SPEC-storage.md`. Whether it is used is decided by CLI flags (`SPEC-cli.md`), not by the config.
10. *(Amendment, from `SPEC-storage.md`)* Docker Compose files feed the overview of how projects connect. They normally live inside each project, so a project can name its Compose file; a Compose file that spans projects (for example one at the repository root) is listed at the top level of the config. The workspace also has its own `INIT-MEMORY.md`, where a person writes down how the services fit together; it is read as context and is not parsed into links.

## Objective

Give Workbench one trustworthy description of the projects it works on, so that every other module (graph, LSP, memory, queries) starts from the same list.

Consumers:
- **`workbench` (CLI)** writes the config and creates each project's memory folder.
- **The MCP server** (`workbench mcp`) reads it to know the projects, their languages, how to activate their environments and how to run their language servers.
- **`lsp` and `platform-setup`** (later modules) read the LSP settings from it.

The module is done when a workspace config round-trips through the engine with validation, a project's memory folder and `INIT-MEMORY.md` are created from a project path, and a query scope can be resolved to a list of projects.

### What the config looks like

```yaml
# .workbench/config.yml
version: 1
lsp:                         # optional workspace-wide defaults, per language
  python:
    server: basedpyright
storage:                     # optional; Workbench's one storage is Memgraph, on your own machine
  host: localhost            # default localhost
  port: 7687                 # default 7687
  namespace: billing-3f9a    # keeps this workspace apart on one Memgraph; written by `workbench init`
  # username: workbench      # optional; Memgraph has no login unless one is set up
  # password_env: MEMGRAPH_PASSWORD   # the variable's name, never the password
compose:                     # optional; Compose files that span projects, for example one at the repository root
  - path: docker-compose.yml # relative to the workspace root, or absolute
    services:                # optional: match a service to a project when its build context cannot be resolved
      billing: billing-api
semantic:                    # optional; used only when the CLI is asked to use an LLM
  llm:
    provider: claude           # copilot-cli | ollama | claude
    model: claude-sonnet-5-5
    api_key_env: ANTHROPIC_API_KEY   # claude only: the variable's name, never the key
projects:
  - id: billing-api            # stable, kebab-case, unique
    path: services/billing     # relative to the workspace root
    language: python
    dependency_manager: uv     # python: poetry | uv; angular: npm | pnpm | yarn | bun
    compose: docker-compose.yml  # optional; path inside the project. Default: the first of compose.yaml,
                                 # compose.yml, docker-compose.yaml, docker-compose.yml at the project root
    environment:               # optional; how to activate the project's environment
      venv: .venv
      activate: uv run
    lsp:                       # optional; overrides the workspace and catalog defaults
      enabled: true
      server: basedpyright
      command: [basedpyright-langserver, --stdio]   # optional launch override
      settings:                # passed to the server as-is
        python.analysis.typeCheckingMode: standard
  - id: web-app
    path: ../shared/web        # may sit outside the workspace root
    language: angular
    dependency_manager: pnpm
```

### Project rules

| Field | Rule |
|---|---|
| `id` | Required. Lowercase letters, digits and `-`; starts with a letter; 1 to 64 characters; unique. Never changes once the memory folder exists. |
| `path` | Required. Relative to the workspace root, or absolute; need not be under the root. Must exist and be a directory when the config is loaded for use. |
| `language` | Required. `python` or `angular`. |
| `dependency_manager` | Required. `poetry` or `uv` for Python; `npm`, `pnpm`, `yarn` or `bun` for Angular (Vite is the build tool, not recorded here). A mismatched pair is an error. |
| `environment.venv` | Optional, Python only. Relative to the project path. |
| `environment.activate` | Optional. A command prefix used to launch tools in the project (for example `uv run` or `poetry run`), including the project's language server. |
| `compose` (project) | Optional. A path relative to the project, or absolute. If absent, the first of `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml` at the project root is used, and having none is fine. A listed path that does not exist is an error. |
| top-level `compose[].path` | Optional list of Compose files that span projects. Relative to the workspace root, or absolute. Must exist. |
| top-level `compose[].services` | Optional map from a Compose service name to a project id, for services whose build context cannot be matched to a project (for example image-only services). Unknown project ids are rejected. |
| `lsp.enabled` | Optional, default `true`. `false` turns the language server off for this project. |
| `lsp.server` | Optional. The server name. Syntax is checked here; the name is checked against the catalog by the `lsp` module. |
| `lsp.command` | Optional. Overrides the catalog's launch command (program and arguments). It runs through `environment.activate` when that is set. |
| `lsp.settings` | Optional. A free-form map passed to the server unchanged; the workspace does not interpret it. |
| top-level `lsp.<language>` | Optional. The same fields (`enabled`, `server`, `command`, `settings`) as workspace-wide defaults for every project of that language. |
| `storage.host` | Optional, default `localhost`. Memgraph runs on the person's own machine; a shared server is not supported. |
| `storage.port` | Optional, default `7687`. |
| `storage.username` | Optional. Memgraph has no login unless one is set up. |
| `storage.password_env` | Optional. The name of an environment variable; the value is held as a `secrecy` secret and read only when a connection is opened. |
| `storage.namespace` | Optional. **The namespace identifies the workspace in the database: one per workspace, unique on a given Memgraph.** It keeps this workspace's data apart from other workspaces on the same local Memgraph. `workbench init` writes a unique one (the folder name plus four random hexadecimal characters); when absent it is `default`, which is only safe for a single workspace. |
| `semantic.llm.provider` | Required when `semantic.llm` is present. One of `copilot-cli`, `ollama`, `claude`. |
| `semantic.llm.model` | Required for `claude` and `ollama`; optional for `copilot-cli`, which uses its own default. |
| `semantic.llm.endpoint` | Optional, `ollama` and `claude` only. For `ollama` the default is `http://localhost:11434`. |
| `semantic.llm.api_key_env` | Required for `claude`, rejected for the other providers. The name of an environment variable; the value is read only when an LLM call is made and is held as a `secrecy` secret. `ollama` needs no key, and `copilot-cli` uses the Copilot CLI's own sign-in. |

**Precedence:** the project's `lsp` block wins over the workspace default for its language, which wins over the catalog default. Fields merge one by one, so a project can override only `settings`.

### Project scope

Every module that queries projects takes a scope: all projects (the default) or a list of project ids. Resolving a scope against the workspace fails with an error naming any unknown id. There is no implicit "current project".

### Per-project memory

`.workbench/memory/<project-id>/INIT-MEMORY.md` exists for every project.

- The folder, `INIT-MEMORY.md` and an empty `annotations.yml` are created when a project is added (also by the CLI's `project add`); an existing `INIT-MEMORY.md` or `annotations.yml` is **never overwritten**.
- The content is seeded from context the project already has: `README.md`, `CLAUDE.md` and `AGENTS.md` at the project root, whichever exist. If none exist, it is an empty template.
- The seed is a template with fixed headings (Purpose, Architecture, How to run, Conventions, Gotchas) followed by an "Existing context" section that quotes each source file under its name, each capped at 20 KB.
- People edit this file. The engine never rewrites it after creation.
- The same folder also holds `annotations.yml`, the mirror of annotations written by people. It is created empty here and kept up to date by `semantic-layer` (`SPEC-semantic-layer.md`). When the storage has lost annotations (a wiped or rebuilt Memgraph), they are replayed from this file, so a person's customisations are kept when the AST is parsed again and when an LLM drafts annotations.

### Workspace memory

`.workbench/memory/INIT-MEMORY.md` is the workspace's own context file: where a person describes how the projects fit together (which service calls which, shared databases and queues, how a request travels). `workbench init` creates it as an empty template with fixed headings (Overview, How the services connect, Shared infrastructure, Conventions). It is never overwritten, it is served to agents with a project's context, and it is **text only**: nothing is parsed from it into links. A link a person wants as an edge in the overview is added as an annotation of kind `link` (`SPEC-cli.md`).

## Tech Stack

- Rust 2024, workspace at `Cargo.toml`.
- `serde` and `yaml_serde` (already workspace dependencies) for the config file.
- `thiserror` for errors (already used).
- No new dependencies are needed for the model. A temp-directory crate for adapter tests is an ask-first item (see Boundaries).
- New workspace member `lib/adapters` (package `adapters`), depending on `engine`, `serde` and `yaml_serde`; `services/cli` (the `workbench` binary, which includes the MCP server) depends on it.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Try it:  cargo run -p workbench -- init --out ./tmp
```

## Project Structure

```
lib/engine/src/
├── workspace/
│   ├── mod.rs          # re-exports
│   ├── project.rs      # Project, ProjectId, Language, DependencyManager, Environment
│   ├── lsp.rs          # LspSettings, per-language defaults and the precedence merge
│   ├── config.rs       # WorkspaceConfig (version, projects) and its validation
│   ├── scope.rs        # ProjectScope and resolution
│   └── memory.rs       # memory layout, INIT-MEMORY template and seeding rules
└── ports.rs            # + WorkspaceStore, MemoryStore, ContextSource

lib/adapters/src/        # shared driven adapters (new crate)
├── lib.rs
├── dir_file.rs         # moved from services/cli: creates .workbench/ and its files
├── workspace_file.rs   # YAML adapter for WorkspaceStore (.workbench/config.yml)
└── memory_dir.rs       # filesystem adapter for MemoryStore and ContextSource
```

The engine has no I/O. `services/cli` (the CLI and the MCP server) picks adapters from `lib/adapters` and wires them to the engine; adapters only that binary needs stay in `services/cli`.

### Ports

| Port | Responsibility |
|---|---|
| `WorkspaceStore` | `load()` returns a validated `WorkspaceConfig`; `save(&config)`; `create_empty()` |
| `MemoryStore` | `create(project_id, seed)` creates the folder and `INIT-MEMORY.md` if absent, never overwrites; `create_workspace()` does the same for the workspace-level file |
| `ContextSource` | `read(project_path)` returns the seed files that exist, each capped at 20 KB |

## Code Style

Parse, don't validate: invalid values cannot be constructed.

```rust
/// A project's stable, kebab-case identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectId(String);

impl FromStr for ProjectId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let ok = (1..=64).contains(&s.len())
            && s.starts_with(|c: char| c.is_ascii_lowercase())
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if ok { Ok(Self(s.to_owned())) } else { Err(Error::InvalidProjectId(s.to_owned())) }
    }
}
```

Conventions follow the existing code: one error enum in `error.rs`, doc comments on public items (`missing_docs` is on), no `unwrap` or `panic` (denied by lints), ports as traits in `ports.rs`, adapters in the service.

## Testing Strategy

- **Framework:** built-in `cargo test`, no extra harness.
- **Engine unit tests** (next to the code, with fake stores like the existing `init` test):
  - LSP precedence: project over workspace default, a partial override keeps the other fields, `enabled: false` is respected.
  - `ProjectId` accepts and rejects the right strings.
  - Duplicate ids, mismatched language and dependency manager, and an unknown version are rejected.
  - Scope resolution: all, a subset, an unknown id.
  - Memory seed: sources found, none found, oversized source is capped.
- **Adapter tests** (in `lib/adapters`): YAML round-trip; a config with a missing `path` fails with a clear error; `MemoryStore` leaves an existing `INIT-MEMORY.md` untouched.
- Coverage target: every rule in the "Project rules" table has a test.

## Boundaries

- **Always:** validate on load and on save; resolve relative paths against the workspace root and accept absolute ones; return errors that name the project id and the field.
- **Ask first:**
  - Adding a dependency (for example a temp-directory crate for adapter tests).
  - Changing the config's `version` or removing a field.
- **Never:** overwrite a person's `INIT-MEMORY.md`; store secrets in the workspace config (they belong in a context config file; the LLM key and the Memgraph password are referenced by environment variable name only); read or write outside `.workbench/` and the project paths.

## Success Criteria

1. `workbench init` writes a `config.yml` that loads back as an empty, valid workspace (`version: 1`, a `storage` block, no projects) and an empty workspace-level `.workbench/memory/INIT-MEMORY.md`.
2. A config with two valid projects, including `lsp` blocks, `compose` entries, a `storage` block and a `semantic.llm` block, loads, validates and saves unchanged.
3. Each rule in the "Project rules" table is rejected with an error that names the project and field when broken.
4. Adding a project creates `.workbench/memory/<id>/INIT-MEMORY.md`, seeded from existing context, and an empty `annotations.yml`, and a second add never overwrites either.
5. The effective LSP settings of a project resolve by the precedence rule (project over workspace default over catalog), field by field.
6. A scope of "all" returns every project; a scope naming an unknown id fails and names it.
7. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

None. The five questions from the first draft are answered and folded into Decisions above.
