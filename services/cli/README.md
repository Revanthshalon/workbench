# workbench

Command-line tool for Workbench, and the way to run and manage the MCP server (`workbench mcp …`, see below). Binary: `workbench`. Before starting `operational-support`, `workbench context check` verifies the connections of the context you will use.

## Usage

```sh
workbench init [--platform <PLATFORM>[,<PLATFORM>...]] [--out <DIR>]
```

Creates `<DIR>/.workbench/config.yml`, empty for now. It refuses to overwrite an existing file.

| Flag | Values | Default |
|------|--------|---------|
| `--platform` | `copilot-cli`, `claude-code`, `codex`; comma-separated or repeated | `copilot-cli,claude-code` |
| `--out` | A directory | `.` (current directory) |

```sh
workbench init --platform copilot-cli,claude-code --out ~/work/my-service
```

## Structure

| Path | Role |
|------|------|
| `src/main.rs` | Wiring only: parse, build the adapter, call `engine`, print |
| `src/cli.rs` | Driving adapter: clap types, mapping to `engine::Platform` |
| `src/adapters/` | Driven adapters implementing `engine` ports (`DirFileStore` implements `ConfigStore`) |
| `src/utils/` | Small helpers (the banner) |
| `src/mcp/` | The MCP server and its life cycle (planned; see `docs/spec/SPEC-core.md`) |

## MCP server

The MCP server has no executable of its own: its code is the `mcp` module of this crate (`src/mcp/`), run with `workbench mcp`.

```sh
workbench context check               # operational-support: check the connections first
workbench mcp start [--mode <mode>]    # run the server in the background
workbench mcp status                   # what is running
workbench mcp logs -f                  # follow the server log
workbench mcp logs --sessions          # operational-support: list per-session logs
workbench mcp restart                  # reload the configs; connected platforms keep working
workbench mcp stop
```

AI platforms are configured to run `workbench mcp connect`, a stdio shim that relays to the running server. The server listens on a Unix socket in `.workbench/run/`, never on the network.

`--mode` picks one bundle of tools, prompts and instructions per server; the default is `developer-support`, which needs no secrets or production access. `operational-support` must be named explicitly and refuses to start without a valid server config. To use several modes, run one server per mode; each has its own socket and log.

| Mode | For | Status |
|------|-----|--------|
| `operational-support` | Incidents: logs → code flow → root cause → fix | MVP |
| `developer-support` | Context for code changes from the service graph and its semantic annotations (kept in the database), combined with your LSP setup, prompts and hooks | MVP (default) |
| `operational-onboarding` | Do's and don'ts for operating the system, from the graph and your runbooks; no production access | planned |
| `developer-onboarding` | A fast guided tour of the codebase | planned |

### MCP surface: `developer-support` (planned)

A project's context (`project_context`: its `INIT-MEMORY.md`, the graph and the annotations), the graph and code tools (`service_map` with `focus=…` for blast radius, `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query`, `callers`, `callees`, `dependents`, `test_map`, `changed_symbols`, `reindex`), the semantic layer (`remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code`), the LSP tools, `edit` tools that change code inside your projects (`apply_patch`, `create_file`, `rename_symbol`, `apply_code_action`), and prompts for changing code (`explain_code`, `plan_change`, `assess_impact`, `implement_change`, `review_change` and more). The full working list is in `docs/spec/capability-map.md`. No secrets and no production access. Keep the index fresh with `workbench watch` and `workbench watch stop`; to have your AI platform run them for you, install its hooks with `workbench platform hooks install`.

### MCP surface: `operational-support` (planned)

| Group | Tools |
|-------|-------|
| Graph and code | `service_map` (boundaries with evidence), `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query` (read-only), `lsp` |
| Operations | `fetch_logs`, `match_log`, `analyze_logs`, `compose_status`, `k8s_status`, `service_config`, `check_config` |
| Data | `db_query`, `db_schema`, `redis_command`, `datastore_health` |
| Semantic layer | `remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code` |
| Index and incidents | `reindex`, `save_incident_report`, `find_incident_reports` |
| Execution flow and change | `error_paths`, `request_timeline`, `recent_changes` |

#### Agent prompts

`diagnose`, `triage_logs`, `explain_flow`, `check_misconfiguration`, `investigate_datastore`, `write_rca`, `enrich_knowledge`.

`write_rca` produces an incident report from [`lib/engine/templates/incident-report.md`](../../lib/engine/templates/incident-report.md): symptoms and how to recognize them, root cause with evidence, a checklist to validate a recurrence, mitigation and fix, a checklist to validate the resolution, prevention, and a runbook entry.
