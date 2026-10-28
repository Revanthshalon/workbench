# Capability Map: workbench-mcp `operational-support`

Status: **approved.** Module specs follow in build order, one at a time, each reviewed before the next.

Scope: ship the `operational-support` mode of `workbench-mcp` completely, plus `workbench` workspace setup. The other modes (`developer-support`, `operational-onboarding`, `developer-onboarding`) are out of scope here, but the shared modules below are designed so those modes can reuse them.

## Decisions this map rests on

- Rust, in the existing workspace, hexagonal layout: ports in `lib/engine`, shared driven adapters in `lib/adapters`, and service-specific adapters in the services.
- MCP over stdio. One process per mode.
- **The default mode is `developer-support`.** `workbench-mcp --mode <mode>` picks a mode; without it the server runs `developer-support`, which needs no secrets and no production access. Operational access is always opt-in by naming `operational-support`.
- **Each mode loads only what it needs.** `developer-support` needs no secrets and no production access, only the graph and LSP. `operational-support` uses everything `developer-support` does (the graph and LSP, to speed up analysis and traversal) *plus* the operational modules and secrets, and requires the server config to exist and be valid *before* the server starts; if not, it exits with an error that says so.
- **Two configurations in one directory.** The `.workbench/` directory holds the configuration for the CLI, the MCP server and the memories, and is git-ignored entirely, so teammates create their own workspace config with `workbench`. The *workspace config* (`.workbench/config.yml`, written by `workbench`) lists the projects of a mono repo or of several repos, with each one's language and dependency manager. The *server config* (`.workbench/mcp/config.yml`) holds the contexts, a required `default_context` (so there is no guessing at start-up) and their credentials. `workbench init` generates the server config as a template (placeholders for credentials; never overwrites an existing one) and adds `.workbench/` to the project's `.gitignore`. A template with unfilled placeholders is not valid, so `operational-support` still refuses to start until it is completed.
- **Secrets:** may be inline in the server config; environment variables override inline values. They are held as `secrecy` secrets from the moment they are read, and never printed, logged or returned by a tool. They are exposed only to open a connection. There is no pattern-based redaction: `secrecy` plus never exposing a value is the whole mechanism. The environment variable naming convention is decided in `SPEC-core.md`.
- **Contexts** are `production`, `staging` and `testing`. All are **read-only**. Each has a code branch and connections to Kubernetes (`kubectl`), PostgreSQL, Redis and Kafka, with credentials.
- **Switching** contexts happens only on an explicit instruction from the user; the agent never switches on its own. A switch asks the user to confirm through MCP elicitation; if the client does not support elicitation, the switch is refused and the user changes `default_context` and restarts. One server process holds all contexts.
- **A workspace is a set of projects.** Each project is an entry in the workspace config with a stable id, a path, a language, a dependency manager and, where needed, a virtual environment or activation command. Projects can be queried together: graph queries and search span every project by default and can be narrowed to one or several. LSP is activated per project and used at workspace level: a request about a file or symbol is routed to the language server of the project that owns it.
- **Per-project memory:** `.workbench/memory/<project-id>/` exists for every project in the workspace, and each starts with an `INIT-MEMORY.md` holding the project's initial context. Adding a project to the workspace creates its folder and file. A new `INIT-MEMORY.md` is seeded from context the project already has (for example its README, `CLAUDE.md` or `AGENTS.md`) and falls back to an empty template; which files count is decided in `SPEC-workspace.md`. To share the memories with a team later, un-ignore `.workbench/memory/` in the project's `.gitignore`.
- **The CLI also builds the AST structures** (`workbench index`) and **updates them incrementally**: only files that changed are re-parsed, never the whole codebase, and what depends on them is re-linked. The MCP `reindex` tool uses the same incremental path.
- The **service graph** is built from the workspace projects with tree-sitter grammars per language. Cross-service connections come from parsing Dockerfiles, Docker Compose files and shared-library APIs.
- **Languages and files:** Python (poetry, uv); Angular with HTML, CSS, SCSS and TypeScript (Vite build; npm, pnpm, yarn or bun); YAML; Dockerfile; Docker Compose.
- The first **log source** is `kubectl` with configurable namespace.
- **Storage is configurable:** SQLite or Memgraph. The audit log lives in the configured backend.
- Every tool call is recorded in an **audit log**, retrievable per session. Nothing is scrubbed by pattern; credentials never reach the log because they are never exposed.
- **LSP at project level and workspace level** is in the first release, in two parts: `workbench-mcp` runs language servers for the workspace's languages and exposes them as tools, and `workbench init <platform>` configures the AI platform's own LSP integration. Both honour per-project settings such as a Python virtual environment, and each project's language server starts only when it is needed. `basedpyright` is the default for Python; the servers for the other languages (Angular and TypeScript, HTML, CSS, SCSS, YAML, Dockerfile, Compose) are chosen in `SPEC-lsp.md`. The LSP settings are part of the workspace config: an optional `lsp` block per project (enabled, server, launch command override, server settings) and optional per-language defaults for the whole workspace; a project's value wins over the workspace default, which wins over the catalog default. The workspace config also says how to activate each project's environment (for Python, a venv path, `poetry run` or `uv run`; for Angular, the project's `node_modules` through its package manager), and the platform LSP config is configurable per project with that activation command; its file format per platform is decided in `SPEC-platform-setup.md`.
- **Resolved execution flows are remembered**, only once you confirm the fix, and combined into a complete orchestration flow of the system. The storage backend is the working store, used to query and combine flows; on confirmation, one file per flow is also written to `.workbench/flows/` and the combined orchestration flow is regenerated there. Because `.workbench/` is git-ignored the files stay local, so sharing flows across a team depends on a shared backend, such as a shared Memgraph. This is part of the first release.
- **Tools are not defined yet.** Tool names below are candidates from the product brief; each module spec defines its tools.

## Modules

| Module id | Responsibility | Tool candidates / artifacts | Depends on |
|---|---|---|---|
| `workspace` | Workspace config model: projects with id, path, language, dependency manager and, where needed, the virtual environment, interpreter or activation command, and the project's LSP settings (plus per-language workspace defaults); project scoping for queries; the per-project memory folder layout (`memory/<project-id>/INIT-MEMORY.md`); read and write | `.workbench/config.yml` format, `.workbench/memory/` layout | — |
| `cli` | `workbench`: create and manage the workspace config; generate the server config template; add projects to the workspace (creating each project's memory folder and `INIT-MEMORY.md`); add `.workbench/` to the project's `.gitignore` | `workbench init` (**built**, config still empty) | `workspace` |
| `core` | MCP server skeleton, required `--mode`, tool and prompt registry, server config loading (inline values, environment overrides, `secrecy`), start-up validation | `workbench-mcp --mode` | `workspace` |
| `contexts` | `production`, `staging`, `testing`: read-only, each with code branch and connections for kubectl, PostgreSQL, Redis and Kafka; one active at a time, switched only on explicit user instruction | context list, switch, current | `core` |
| `storage` | Persistence ports with SQLite and Memgraph adapters, chosen by config | — | `core` |
| `audit` | Append-only record of every tool call, with its session and context | session log retrieval | `core`, `storage` |
| `lsp` | Catalog of language servers per language (default for Python: basedpyright). Two entry points on one shared launcher: `workbench lsp run <project-id>` runs the project's activation command (venv, `poetry run`, `uv run`, Vite project) and execs its language server over stdio; `workbench-mcp` starts the same servers lazily and exposes them as tools. Workspace-level requests are routed to the owning project's server | `workbench lsp run <project-id>`; MCP `lsp` tools (to be defined) | `workspace`, `core` |
| `platform-setup` | `workbench init <platform>` configures the AI platform's own LSP integration (Copilot CLI, Claude Code, Codex) from the workspace config, pointing each project's entry at `workbench lsp run <project-id>` and honouring the LSP settings in the workspace config; configurable per project and per folder, with an activation command and the folder's dependency manager | `workbench init --platform …` (extended) | `cli`, `lsp` |
| `index` | `workbench index`: builds the per-file AST data and updates it incrementally from changes (content hashes, with `git diff` as a shortcut); runs on demand, from git hooks or from a file watcher | `workbench index [--project …] [--changed]` | `cli`, `graph` |
| `graph` | Tree-sitter parsing (Python, TypeScript, HTML, CSS, SCSS, YAML, Dockerfile, Compose); Dockerfile, Compose and shared-lib API cross-service links; service boundaries with evidence; queries across all projects or a chosen subset; indexing | `service_map`, `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query`, `reindex` (each accepting a project scope, default all) | `workspace`, `storage` |
| `semantic-layer` | Remembered knowledge and code search across projects, seeded from each project's `INIT-MEMORY.md` | `remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code` | `storage`, `graph` |
| `operations` | Service logs, runtime and config in the active context; first adapter is `kubectl` | `fetch_logs`, `match_log`, `analyze_logs`, `compose_status`, `k8s_status`, `service_config`, `check_config` | `contexts`, `audit`, `graph`, `lsp` |
| `data` | PostgreSQL, Redis and Kafka access and health, read-only, in the active context; Kafka covers topics, consumer-group lag and message contents | `db_query`, `db_schema`, `redis_command`, `datastore_health`, Kafka tools (to be defined) | `contexts`, `audit` |
| `execution-flow` | Failure paths, request timelines, change history, blast radius | `error_paths`, `request_timeline`, `recent_changes`, `service_map(focus=…)` | `graph`, `lsp`, `operations` |
| `flow-memory` | Stores each execution flow whose fix you confirmed, and combines them into a complete orchestration flow of the system; kept as local files and in the storage backend | save and recall flow, orchestration map, flow files (to be defined) | `storage`, `audit`, `execution-flow`, `semantic-layer` |
| `incidents` | Incident reports and recall of past ones | `save_incident_report`, `find_incident_reports`, `templates/incident-report.md` | `storage`, `semantic-layer` |
| `agent-prompts` | Prompts that drive the tools | `diagnose`, `triage_logs`, `explain_flow`, `check_misconfiguration`, `investigate_datastore`, `write_rca`, `enrich_knowledge` | all of the above |

Build order:

```
workspace → cli, core → contexts, storage, lsp → audit → graph
          → semantic-layer, operations, data → execution-flow → flow-memory, incidents → agent-prompts

platform-setup: any time after lsp and cli
index: any time after graph and cli (the incremental logic itself lives in graph)
```

## Shared with the other modes

- **Used by `developer-support`**, the default mode (no secrets, no production access): `workspace`, `core`, `storage`, `graph`, `semantic-layer`, `lsp`, and `platform-setup` and `index` on the CLI side. It must start without a server config.
- **Reused by the onboarding modes:** `graph`, `semantic-layer`, `lsp`, and the read paths of `incidents` and `flow-memory`.
- **`operational-support` uses all of the above** (graph, semantic layer and LSP, for faster analysis and traversal) **plus** the operational-only modules: `contexts`, `audit`, `operations`, `data`, `execution-flow`, `flow-memory`, `incidents`, `agent-prompts`.

## Boundaries between modules

- `workspace` is the only contract between `cli` and `workbench-mcp`: the CLI writes it (config and memory folders), the server reads it.
- Project scope is a `workspace` concept: every module that queries projects takes a project id list and defaults to all of them.
- `INIT-MEMORY.md` is human-editable context. `semantic-layer` reads it and may suggest additions, but never overwrites what a person wrote.
- Only `contexts` reads credentials and hands resolved settings to tools; tools never read secrets themselves. Secrets are `secrecy` values everywhere and are exposed only at the point a connection is opened.
- The agent never switches context unless the user explicitly instructs it; the `contexts` spec defines how the server checks that.
- Read-only is an invariant of every context, not a production special case. `operations` and `data` specs must say how it is enforced (for example a SQL allowlist, or a read-only role) and test it.
- `storage` exposes ports; `graph`, `semantic-layer`, `audit`, `flow-memory` and `incidents` do not know whether the backend is SQLite or Memgraph.
- Incremental indexing lives in `graph` and runs through `engine` ports, so `workbench index` and the MCP `reindex` tool share one code path. Parsed data is keyed by project and file content hash; an unchanged file is never parsed again. The CLI and the server may touch the same storage, so the `storage` spec must define safe concurrent access (for example SQLite in WAL mode).
- `graph` owns what a "service boundary with evidence" is; `operations`, `execution-flow` and `semantic-layer` consume it.
- `audit` records every tool call with its session and the active context. Retrieval is per session. The session boundary is defined in its spec.
- `flow-memory` builds on the `audit` trail of a session and on `execution-flow` results; it does not call production systems itself. It saves a flow only after the user confirms the fix.
- Kafka reads must not join a consumer group or commit offsets, so reading never changes the system's state.
- `lsp` runs per project using the language and environment in the workspace config; it needs no secrets. `platform-setup` reuses its language server catalog, so the server and the platform use the same servers and settings.

## Open questions

None at map level. Each module spec will list its own.
