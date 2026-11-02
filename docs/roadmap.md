# Implementation roadmap

How Workbench is built, in order, and which document to open for each step. The order follows the build order and dependencies in the [capability map](spec/capability-map.md); where this page and a spec disagree, the spec wins.

## Rules

- **A milestone starts when every spec it needs is approved.** Its plan and tasks are then written (`tasks/plan.md` and `tasks/todo.md` for M1; one pair per later milestone, named `tasks/mN-plan.md` and `tasks/mN-todo.md`).
- **Every milestone leaves `develop` building:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- **New dependencies** are added by the task that first uses them, with a pinned version and `cargo deny check`, and only if approved in the spec named below.
- **Statuses:** *approved* = merged to `develop` (every module spec is, see [`spec/README.md`](spec/README.md)); *not written* = no spec yet.

## Milestones

| | Milestone | Delivers | Specs to read | Status | Needs |
|---|---|---|---|---|---|
| **M1** | Workspace and CLI foundation | `workbench init`, `project add \| list \| remove`, workspace discovery; the whole config model, memory folders, YAML store | [`SPEC-workspace`](spec/SPEC-workspace.md), [`SPEC-cli`](spec/SPEC-cli.md) (`init`, `project`, common behaviour), [`language-framework-binaries`](spec/language-framework-binaries.md), [LLD](lld/LLD-workspace-cli.md), [plan](../tasks/plan.md), [tasks](../tasks/todo.md), [ADR-0001](adr/0001-engine-free-of-serde.md), [ADR-0002](adr/0002-no-toml-crate-for-pyproject.md) | approved; plan and tasks written | none |
| **M2** | Server core | `workbench mcp start \| serve \| stop \| restart \| status \| logs \| connect`; modes, tool and prompt registry, server config, secrets; `init` also writes the `.workbench/mcp/` templates | [`SPEC-core`](spec/SPEC-core.md), [`SPEC-cli`](spec/SPEC-cli.md) (`mcp` group, `init` templates) | approved | M1; dependencies below |
| **M3** | Foundations, three tracks in parallel | **storage:** Memgraph adapter, migrations, `storage status`. **contexts:** context files, SSH remote runner, session switching. **lsp:** language-server catalog and launcher, `lsp run \| list \| check` | [`SPEC-storage`](spec/SPEC-storage.md) + [ADR-0003](adr/0003-memgraph-as-only-storage.md); [`SPEC-contexts`](spec/SPEC-contexts.md); [`SPEC-lsp`](spec/SPEC-lsp.md) | approved | M2; dependencies below (storage, contexts, lsp rows) |
| **M4** | Audit and platform setup | per-session audit log and its retrieval; `platform setup` and `platform hooks` | [`SPEC-audit`](spec/SPEC-audit.md); `SPEC-platform-setup` | audit approved; platform-setup **not written** | M3 (lsp); write `SPEC-platform-setup` |
| **M5** | Graph and indexing | tree-sitter graph, `graph_query`, `index`, `watch`; wires `context check \| sync` (their `code` check needs an index) | [`SPEC-graph`](spec/SPEC-graph.md), [`SPEC-index`](spec/SPEC-index.md), [`SPEC-cli`](spec/SPEC-cli.md) (`index`, `watch`, `context`) | approved | M3 (storage); dependencies below |
| **M6** | Semantic layer | annotations on AST nodes, the `annotations.yml` mirror, optional LLM drafting, `semantic` commands | [`SPEC-semantic-layer`](spec/SPEC-semantic-layer.md), [`SPEC-cli`](spec/SPEC-cli.md) (`semantic`) | approved | M5; `reqwest` is already approved |
| **M7** | developer-support complete | prompts, tools and server instructions for changing code; `edit` tools; the default mode is usable | `SPEC-dev-support` | **not written** | M2, M3, M5, M6, M4 (platform setup); write `SPEC-dev-support` |
| **M8** | Operational tools | logs, kubectl, config checks; PostgreSQL, Redis and Kafka read-only access | [`SPEC-operations`](spec/SPEC-operations.md), [`SPEC-data`](spec/SPEC-data.md) | approved | M3 (contexts), M4 (audit), M5; dependencies below |
| **M9** | Flows, incidents and prompts | execution flows, flow memory, incident reports, the seven operational prompts; `operational-support` complete | [`SPEC-execution-flow`](spec/SPEC-execution-flow.md), [`SPEC-flow-memory`](spec/SPEC-flow-memory.md), [`SPEC-incidents`](spec/SPEC-incidents.md), [`SPEC-agent-prompts`](spec/SPEC-agent-prompts.md) | approved | M6, M8 |

**Releases.** `developer-support` (the default mode) is complete after **M7**. `operational-support` is complete after **M9** and also includes everything in M7.

```
M1 ─ M2 ─┬─ M3 storage ───────────┐
         ├─ M3 contexts ──────────┼─ M5 graph, index ─ M6 semantic ─ M7 dev-support ─┐
         └─ M3 lsp ─ M4 audit,    │                                                    ├─ done: developer-support
                       platform ──┘                                                    │
                       M5 ─ M8 operations, data ─ M9 flows, incidents, prompts ───────┴─ done: operational-support
```

## Dependencies to approve before each milestone

Each is added by the first task that uses it, with a pinned version and `cargo deny check` (`deny.toml` allows only MIT, Apache-2.0 and Unicode-3.0; a needed exception is a reviewed change with a reason). Approved already: `tempfile` (dev only, M1), `notify` and `reqwest` (`SPEC-cli`).

| Milestone | To approve | Named in |
|---|---|---|
| M2 | `rmcp`; the tokio features `net`, `io-util`, `signal`, `process`, `fs`, `time`; `static_assertions` (optional); a logging crate (optional) | [`SPEC-core`](spec/SPEC-core.md) |
| M3 storage | a Bolt client for Memgraph (for example `neo4rs`); a Memgraph container in CI and a compose file or Makefile target | [`SPEC-storage`](spec/SPEC-storage.md) |
| M3 contexts | an SSH library (for example `russh`); PostgreSQL (`tokio-postgres`), Redis and Kafka clients (`rdkafka` needs `cmake`, which CI installs; or `rskafka`) | [`SPEC-contexts`](spec/SPEC-contexts.md), [`SPEC-data`](spec/SPEC-data.md) |
| M3 lsp | LSP types (`lsp-types`); a JSON-RPC library only if not written in-house | [`SPEC-lsp`](spec/SPEC-lsp.md) |
| M4 | `serde_json`, `time`, `sha2` | [`SPEC-audit`](spec/SPEC-audit.md) |
| M5 | `tree-sitter` and one grammar per language; a YAML parser for Compose if `yaml_serde` is not enough; `ignore` (`.gitignore` walking); a file-lock crate only if the standard library's is missing | [`SPEC-graph`](spec/SPEC-graph.md), [`SPEC-index`](spec/SPEC-index.md) |
| M8 | `regex` (operations); a SQL parser (for example `sqlparser`) and the Kafka and Redis clients from M3 (data) | [`SPEC-operations`](spec/SPEC-operations.md), [`SPEC-data`](spec/SPEC-data.md) |

## Where to look

| Question | Document |
|---|---|
| What is Workbench and what is in scope? | [`README.md`](../README.md), [capability map](spec/capability-map.md) |
| How do the parts fit together? | [HLD](hld/HLD-workbench.md) |
| What does a module do, exactly? | its spec in [`docs/spec`](spec/README.md) |
| Which command takes which options? | [CLI reference](reference/cli.md) |
| Why was it decided this way? | [ADRs](adr/README.md) |
| How is the current milestone built? | the LLD and the milestone's `tasks/` files |
| What is the order and what blocks what? | this page |
