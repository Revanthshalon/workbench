# Implementation roadmap

How Workbench is built, in order, and which document to open for each step. The order follows the build order and dependencies in the [capability map](spec/capability-map.md); where this page and a spec disagree, the spec wins.

## Rules

- **A milestone starts when every spec it needs is approved.** Its plan and tasks are then written (`tasks/plan.md` and `tasks/todo.md` for M1; one pair per later milestone, named `tasks/mN-plan.md` and `tasks/mN-todo.md`).
- **Every milestone leaves `develop` building:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- **New dependencies** are added by the task that first uses them, with a pinned version and `cargo deny check`, and only if approved in the spec named below.
- **Statuses:** *approved* = merged to `develop`; *draft* = open PR, stacked (#5 to #15, see [`spec/README.md`](spec/README.md)); *not written* = no spec yet.

## Milestones

| | Milestone | Delivers | Specs to read | Status | Needs |
|---|---|---|---|---|---|
| **M1** | Workspace and CLI foundation | `workbench init`, `project add \| list \| remove`, workspace discovery; the whole config model, memory folders, YAML store | [`SPEC-workspace`](spec/SPEC-workspace.md), [`SPEC-cli`](spec/SPEC-cli.md) (`init`, `project`, common behaviour), [`language-framework-binaries`](spec/language-framework-binaries.md), [LLD](lld/LLD-workspace-cli.md), [plan](../tasks/plan.md), [tasks](../tasks/todo.md), [ADR-0001](adr/0001-engine-free-of-serde.md), [ADR-0002](adr/0002-no-toml-crate-for-pyproject.md) | approved; plan and tasks written | none |
| **M2** | Server core | `workbench mcp start \| serve \| stop \| restart \| status \| logs \| connect`; modes, tool and prompt registry, server config, secrets; `init` also writes the `.workbench/mcp/` templates | [`SPEC-core`](spec/SPEC-core.md), [`SPEC-cli`](spec/SPEC-cli.md) (`mcp` group, `init` templates) | approved | M1; approve `rmcp` and the extra tokio features |
| **M3** | Foundations, three tracks in parallel | **storage:** Memgraph adapter, migrations, `storage status`. **contexts:** context files, SSH remote runner, session switching. **lsp:** language-server catalog and launcher, `lsp run \| list \| check` | [`SPEC-storage`](spec/SPEC-storage.md) + [ADR-0003](adr/0003-memgraph-as-only-storage.md); [`SPEC-contexts`](spec/SPEC-contexts.md); `SPEC-lsp` | storage, contexts approved; lsp draft (#5) | M2; approve the Bolt client, the SSH library, PostgreSQL, Redis and Kafka clients, and a Memgraph container in CI |
| **M4** | Audit and platform setup | per-session audit log and its retrieval; `platform setup` and `platform hooks` | `SPEC-audit`; `SPEC-platform-setup` | audit draft (#6); platform-setup not written | M3 (lsp); write `SPEC-platform-setup` |
| **M5** | Graph and indexing | tree-sitter graph, `graph_query`, `index`, `watch`; wires `context check \| sync` (their `code` check needs an index) | `SPEC-graph`, `SPEC-index`, [`SPEC-cli`](spec/SPEC-cli.md) (`index`, `watch`, `context`) | graph draft (#7), index draft (#9) | M3 (storage); approve the tree-sitter grammars and `notify` |
| **M6** | Semantic layer | annotations on AST nodes, the `annotations.yml` mirror, optional LLM drafting, `semantic` commands | `SPEC-semantic-layer`, [`SPEC-cli`](spec/SPEC-cli.md) (`semantic`) | draft (#8) | M5; `reqwest` is approved |
| **M7** | developer-support complete | prompts, tools and server instructions for changing code; `edit` tools; the default mode is usable | `SPEC-dev-support` | **not written** | M2, M3, M5, M6, M4 (platform setup); write `SPEC-dev-support` |
| **M8** | Operational tools | logs, kubectl, config checks; PostgreSQL, Redis and Kafka read-only access | `SPEC-operations`, `SPEC-data` | drafts (#10, #11) | M3 (contexts), M4 (audit), M5 |
| **M9** | Flows, incidents and prompts | execution flows, flow memory, incident reports, the seven operational prompts; `operational-support` complete | `SPEC-execution-flow`, `SPEC-flow-memory`, `SPEC-incidents`, `SPEC-agent-prompts` | drafts (#12 to #15) | M6, M8 |

**Releases.** `developer-support` (the default mode) is complete after **M7**. `operational-support` is complete after **M9** and also includes everything in M7.

```
M1 ─ M2 ─┬─ M3 storage ───────────┐
         ├─ M3 contexts ──────────┼─ M5 graph, index ─ M6 semantic ─ M7 dev-support ─┐
         └─ M3 lsp ─ M4 audit,    │                                                    ├─ done: developer-support
                       platform ──┘                                                    │
                       M5 ─ M8 operations, data ─ M9 flows, incidents, prompts ───────┴─ done: operational-support
```

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
