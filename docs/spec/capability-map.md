# Capability Map: workbench-mcp `operational-support`

Status: **approved (v8).** Module specs follow in build order, one at a time, each reviewed before the next.

Scope: ship the `operational-support` mode of `workbench-mcp` completely, plus `workbench-cli` workspace setup. The other modes (`developer-support`, `operational-onboarding`, `developer-onboarding`) are out of scope here, but the shared modules below are designed so those modes can reuse them.

## Decisions this map rests on

- Rust, in the existing workspace, hexagonal layout: ports in `lib/engine`, adapters in the services.
- MCP over stdio. One process per mode.
- **The default mode is `developer-support`.** `workbench-mcp --mode <mode>` picks a mode; without it the server runs `developer-support`, which needs no secrets and no production access. Operational access is always opt-in by naming `operational-support`.
- **Each mode loads only what it needs.** `developer-support` needs no secrets and no production access, only the graph and LSP. `operational-support` uses everything `developer-support` does (the graph and LSP, to speed up analysis and traversal) *plus* the operational modules and secrets, and requires the server config to exist and be valid *before* the server starts; if not, it exits with an error that says so.
- **Two configurations in the same directory.** The *workspace config* (`.workbench-cli/config.yml`, written by `workbench-cli`) lists the folders of a project, mono repo or multiple repos, with each one's language and dependency manager. The *server config* (`.workbench-cli/mcp/config.yml`) holds the contexts and their credentials. `workbench-cli init` generates the server config as a template (contexts, `default_context`, placeholders for credentials; never overwrites an existing one) and writes a `.gitignore` for `.workbench-cli/mcp/`. A template with unfilled placeholders is not valid, so `operational-support` still refuses to start until it is completed.
- **Secrets:** may be inline in the server config; environment variables override inline values. They are held as `secrecy` secrets from the moment they are read, and never printed, logged or returned by a tool. They are exposed only to open a connection.
- **Contexts** are `production`, `staging` and `testing`. All are **read-only**. Each has a code branch and connections to Kubernetes (`kubectl`), PostgreSQL, Redis and Kafka, with credentials.
- **Switching** contexts happens only on an explicit instruction from the user; the agent never switches on its own. One server process holds all contexts.
- The **service graph** is built from the workspace folders with tree-sitter grammars per language. Cross-service connections come from parsing Dockerfiles, Docker Compose files and shared-library APIs.
- **Languages and files:** Python (poetry, uv); Angular with HTML, CSS, SCSS and TypeScript (Vite); YAML; Dockerfile; Docker Compose.
- The first **log source** is `kubectl`.
- **Storage is configurable:** SQLite or Memgraph.
- Every tool call is recorded in an **audit log**, retrievable per session. Nothing is scrubbed by pattern; credentials never reach the log because they are never exposed.
- **LSP at project level** is in the first release, in two parts: `workbench-mcp` runs language servers for the workspace's languages and exposes them as tools, and `workbench-cli init <platform>` configures the AI platform's own LSP integration. Both honour per-folder settings such as a Python virtual environment.
- **Resolved execution flows are remembered**, only once you confirm the fix, and combined into a complete orchestration flow of the system. They are kept both as local files under `.workbench-cli/flows/` (git-ignored, never committed) and in the storage backend. This is part of the first release.
- **Tools are not defined yet.** Tool names below are candidates from the product brief; each module spec defines its tools.

## Modules

| Module id | Responsibility | Tool candidates / artifacts | Depends on |
|---|---|---|---|
| `workspace` | Workspace config model: folders, language, dependency manager and, where needed, the virtual environment, interpreter or activation command per folder; read and write | `.workbench-cli/config.yml` format | — |
| `cli` | `workbench-cli`: create and manage the workspace config; generate the server config template; write the `.gitignore` for `mcp/` and `flows/` | `workbench-cli init` (**built**, config still empty) | `workspace` |
| `core` | MCP server skeleton, required `--mode`, tool and prompt registry, server config loading (inline values, environment overrides, `secrecy`), start-up validation | `workbench-mcp --mode` | `workspace` |
| `contexts` | `production`, `staging`, `testing`: read-only, each with code branch and connections for kubectl, PostgreSQL, Redis and Kafka; one active at a time, switched only on explicit user instruction | context list, switch, current | `core` |
| `storage` | Persistence ports with SQLite and Memgraph adapters, chosen by config | — | `core` |
| `audit` | Append-only record of every tool call, with its session and context | session log retrieval | `core`, `storage` |
| `lsp` | Catalog of language servers per language (default for Python: basedpyright); launches them per workspace folder, venv-aware and dependency-manager-aware (poetry, uv, Vite), and exposes them as tools | `lsp` (to be defined) | `workspace`, `core` |
| `platform-setup` | `workbench-cli init <platform>` configures the AI platform's own LSP integration (Copilot CLI, Claude Code, Codex) from the workspace config; configurable per project and per folder, with an activation command and the folder's dependency manager | `workbench-cli init --platform …` (extended) | `cli`, `lsp` |
| `graph` | Tree-sitter parsing (Python, TypeScript, HTML, CSS, SCSS, YAML, Dockerfile, Compose); Dockerfile, Compose and shared-lib API cross-service links; service boundaries with evidence; queries; indexing | `service_map`, `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query`, `reindex` | `workspace`, `storage` |
| `semantic-layer` | Remembered knowledge and code search | `remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code` | `storage`, `graph` |
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
```

## Shared with the other modes

- **Used by `developer-support`**, the default mode (no secrets, no production access): `workspace`, `core`, `storage`, `graph`, `semantic-layer`, `lsp`, and `platform-setup` on the CLI side. It must start without a server config.
- **Reused by the onboarding modes:** `graph`, `semantic-layer`, `lsp`, and the read paths of `incidents` and `flow-memory`.
- **`operational-support` uses all of the above** (graph, semantic layer and LSP, for faster analysis and traversal) **plus** the operational-only modules: `contexts`, `audit`, `operations`, `data`, `execution-flow`, `flow-memory`, `incidents`, `agent-prompts`.

## Boundaries between modules

- `workspace` is the only contract between `cli` and `workbench-mcp`: the CLI writes it, the server reads it.
- Only `contexts` reads credentials and hands resolved settings to tools; tools never read secrets themselves. Secrets are `secrecy` values everywhere and are exposed only at the point a connection is opened.
- The agent never switches context unless the user explicitly instructs it; the `contexts` spec defines how the server checks that.
- Read-only is an invariant of every context, not a production special case. `operations` and `data` specs must say how it is enforced (for example a SQL allowlist, or a read-only role) and test it.
- `storage` exposes ports; `graph`, `semantic-layer`, `audit`, `flow-memory` and `incidents` do not know whether the backend is SQLite or Memgraph.
- `graph` owns what a "service boundary with evidence" is; `operations`, `execution-flow` and `semantic-layer` consume it.
- `audit` records every tool call with its session and the active context. Retrieval is per session. The session boundary is defined in its spec.
- `flow-memory` builds on the `audit` trail of a session and on `execution-flow` results; it does not call production systems itself. It saves a flow only after the user confirms the fix.
- Kafka reads must not join a consumer group or commit offsets, so reading never changes the system's state.
- `lsp` runs per workspace folder using the languages and environments in the workspace config; it needs no secrets. `platform-setup` reuses its language server catalog, so the server and the platform use the same servers and settings.

## My calls (change any)

- Server config path `.workbench-cli/mcp/config.yml`, with the `.gitignore` (confirmed). `workbench-cli init` auto-generates the server config as a template (confirmed).
- The audit log lives in the configured storage backend (confirmed).
- No pattern-based redaction: `secrecy` plus never exposing a value is the whole mechanism (confirmed).
- The server config names a required `default_context`, so there is no guessing at start-up.
- A context switch asks you to confirm through MCP elicitation. If the client does not support elicitation, the switch is refused and you change `default_context` and restart.
- Kafka message contents are readable (confirmed), without joining a consumer group or committing offsets.
- Flow memory: the storage backend is the working store, used to query and combine flows. When you confirm a fix, one file per flow is also written to `.workbench-cli/flows/` and the combined orchestration flow is regenerated there. The folder is git-ignored (confirmed), so the files stay local. Sharing flows across a team therefore depends on a shared backend, such as a shared Memgraph. `workbench-cli init` writes the `.gitignore` entry.
- Language servers: `basedpyright` is the default for Python. The other languages (Angular and TypeScript, HTML, CSS, SCSS, YAML, Dockerfile, Compose) are chosen in `SPEC-lsp.md`.
- Launching a language server takes the folder's dependency manager into account: the workspace config says how to activate the environment (for Python, a venv path, or `poetry run` or `uv run`; for Angular, the Vite project's `node_modules`).
- Platform LSP config is written by `workbench-cli init <platform>` and is configurable per project and per folder, with an activation command. The exact file format per platform is decided in `SPEC-platform-setup.md`.
- The environment variable naming convention is decided in `SPEC-core.md`.

## Open questions

None at map level. Each module spec will list its own.
