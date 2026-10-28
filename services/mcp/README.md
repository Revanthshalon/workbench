# workbench-mcp

The Workbench MCP server, as a library crate. It has no executable of its own: the `workbench` CLI runs it with `workbench mcp`.

```sh
workbench mcp start [--mode <mode>]    # run the server in the background
workbench mcp status                   # what is running
workbench mcp logs -f                  # follow the log
workbench mcp restart                  # reload the configs; connected platforms keep working
workbench mcp stop
```

AI platforms are configured to run `workbench mcp connect`, a stdio shim that relays to the running server. The server listens on a Unix socket in `.workbench/run/`, never on the network.

`--mode` picks one bundle of tools, prompts and instructions per server; the default is `developer-support`, which needs no secrets or production access. `operational-support` must be named explicitly and refuses to start without a valid server config. To use several modes, run one server per mode; each has its own socket and log.

| Mode | For | Status |
|------|-----|--------|
| `operational-support` | Incidents: logs → code flow → root cause → fix | MVP |
| `developer-support` | Context for code changes from the service graph, combined with your LSP setup, context files, prompts and hooks | planned (default) |
| `operational-onboarding` | Do's and don'ts for operating the system, from the graph and your runbooks; no production access | planned |
| `developer-onboarding` | A fast guided tour of the codebase | planned |

## MCP surface: `operational-support` (planned)

| Group | Tools |
|-------|-------|
| Graph and code | `service_map` (boundaries with evidence), `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query` (read-only), `lsp` |
| Operations | `fetch_logs`, `match_log`, `analyze_logs`, `compose_status`, `k8s_status`, `service_config`, `check_config` |
| Data | `db_query`, `db_schema`, `redis_command`, `datastore_health` |
| Semantic layer | `remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code` |
| Index and incidents | `reindex`, `save_incident_report`, `find_incident_reports` |
| Execution flow and change | `error_paths`, `request_timeline`, `recent_changes`; `service_map(focus=…)` for blast radius |

### Agent prompts

`diagnose`, `triage_logs`, `explain_flow`, `check_misconfiguration`, `investigate_datastore`, `write_rca`, `enrich_knowledge`.

`write_rca` produces an incident report from [`templates/incident-report.md`](templates/incident-report.md): symptoms and how to recognize them, root cause with evidence, a checklist to validate a recurrence, mitigation and fix, a checklist to validate the resolution, prevention, and a runbook entry.
