# High-level design: Workbench

Status: **draft for review.** Summarizes the approved [capability map](../spec/capability-map.md) and module specs at system level. Where this document and a spec disagree, the spec wins; fix this document. Component-level design for the first build slice is in [`LLD-workspace-cli.md`](../lld/LLD-workspace-cli.md).

## 1. Purpose and scope

Workbench turns a system's code, logs, configuration and runbooks into tools an agent can use to **change** code (`developer-support`, the default) and to **investigate** incidents (`operational-support`). It is one person's local tool: everything runs on their machine, and nothing listens on the network.

In scope for this design: the `developer-support` and `operational-support` modes and the `workbench` CLI that sets up and maintains a workspace. Out of scope: the onboarding modes and any shared or hosted deployment.

## 2. Context

```
   AI platform (Claude Code, Copilot CLI, Codex)
        │ stdio                          │ own LSP integration
        ▼                                ▼
 workbench mcp connect  (shim)     workbench lsp run <project>
        │ Unix socket (0600)              │
        ▼                                 ▼
 ┌────────────────────────────── workbench (one binary) ─────────────────────────┐
 │  CLI (clap)        MCP server (background, one per workspace and mode)         │
 │        └──────────── driving adapters ──────────────┐                          │
 │                                                     ▼                          │
 │                                           engine: domain + ports + use cases   │
 │                                                     ▲                          │
 │        driven adapters: files, Memgraph, git, LSP, SSH/kubectl/DB/Redis/Kafka  │
 └─────────────────────────────────────────────────────────────────────────────────┘
        │                  │                       │                    │
   .workbench/        project checkouts       Memgraph (local)     context VMs (SSH)
   (config, memory)   and branch worktrees    one graph per view   read-only, prod/stage/test
```

Actors: the **developer** (runs the CLI, writes memory and annotations), the **agent** (calls MCP tools), and, in `operational-support`, the **context VMs** reached over SSH.

## 3. Key decisions

| # | Decision | Where it is decided |
|---|---|---|
| 1 | **One binary** `workbench`; the MCP server is its `mcp` command group, run as a **background server** per workspace and mode on a Unix socket. Platforms run the stdio shim `mcp connect`, which reconnects for up to 90 s across restarts. | `SPEC-core.md` |
| 2 | **Hexagonal layout.** `lib/engine` holds domain, ports and use cases and does no I/O; `lib/adapters` holds shared driven adapters; `services/cli` holds the driving adapters (CLI, MCP) and adapters only it needs. | capability map |
| 3 | **Default mode is `developer-support`**: needs no secrets and no production access, only the graph and LSP. Operational access is opt-in with `--mode operational-support`. | capability map, `SPEC-core.md` |
| 4 | **One storage: Memgraph**, on the person's own machine; Workbench never installs or starts it. One database per workspace (a `namespace`), one graph per **view** (the working trees, and each context's branches). Queries cross projects, never views or namespaces. | `SPEC-storage.md` |
| 5 | **Configuration split:** workspace config (`.workbench/config.yml`: projects, storage, LSP, optional LLM) is read by CLI and server; server config and per-context files (`.workbench/mcp/…`) hold contexts and credentials and are required only by `operational-support`. | `SPEC-workspace.md`, `SPEC-core.md` |
| 6 | **Secrets** are referenced by environment-variable name or written in a context file, held as `secrecy` secrets, exposed only to open a connection, never logged. No pattern-based redaction. | `SPEC-core.md` |
| 7 | **Contexts** `production`, `staging`, `testing` are read-only, each with its own VM, branch and credentials; one active per session, switched only on explicit user instruction with confirmation. Code analysis uses a separate git worktree per project and context, so no working tree is touched. | `SPEC-contexts.md` |
| 8 | **Incremental indexing** (content hashes, `git diff` as shortcut) shared by `workbench index`, the watcher and MCP `reindex`, behind one write lock. A **watcher** runs as a detached background process; platform hooks for it are installed only on request. | `SPEC-cli.md`, `SPEC-index.md` (to write) |
| 9 | **Semantic annotations** (purpose, role, domain term, tag, link) on AST nodes, by people and agents; person-written ones are mirrored to `annotations.yml` and replayed if Memgraph is rebuilt. **LLM drafting is opt-in twice** (config and flag). | `SPEC-semantic-layer.md` (to write) |
| 10 | **Audit:** in `operational-support` every session has an append-only JSON Lines log; it is the audit record. | `SPEC-core.md` |

## 4. Runtime view

- **Developer setup** (CLI only): `init` → `project add` (detect language and manager, create memory) → `index` → optional `semantic add`. Needs Memgraph only for `index` and later.
- **developer-support session:** platform → `mcp connect` → server → engine → Memgraph (graph, annotations), LSP servers started lazily per project, `edit` tools writing into project directories (the one mode that does).
- **operational-support session:** as above, plus the start context's connection check at start-up, per-session active context, tools reaching kubectl, PostgreSQL, Redis and Kafka through the context's VM, every call written to the session log.
- **Watcher:** `watch` starts a detached process; a thread inside batches file events (500 ms) and runs the incremental index; `flush` makes the next query see a just-made edit.

## 5. Data and files

```
.workbench/                  git-ignored entirely
├── config.yml               workspace config (version, storage, projects, …)
├── memory/
│   ├── INIT-MEMORY.md       workspace context (text only)
│   └── <project-id>/        INIT-MEMORY.md, annotations.yml (mirror of person-written annotations)
├── mcp/
│   ├── config.yml           server config (contexts, default_context, logs.dir)
│   └── contexts/<name>.yml  per-context branch, connections, credentials
├── run/                     sockets and pid files (dir 0700, socket 0600)
├── logs/                    server, watcher and session logs (dir 0700, files 0600)
├── worktrees/<context>/<project-id>/   branch checkouts for operational analysis
└── flows/                   one file per confirmed execution flow
```

Memgraph holds, per namespace and view: `Project` nodes, code nodes, derived cross-project overview (from Compose, Dockerfiles, shared APIs), annotations, schema versions. Nodes carry `namespace`, `view`, `project`.

## 6. Modules and build order

| Module | Responsibility (one line) | Depends on |
|---|---|---|
| `workspace` | Workspace config model, validation, memory layout, project scope | — |
| `cli` | `workbench` command surface; `init`, `project`, hosts `index`/`semantic` groups | `workspace` (+ others per command group) |
| `core` | MCP server life cycle, modes, registries, server config, logs | `workspace` |
| `contexts` | Contexts, connection checks, branch worktrees | `core` |
| `storage` | Memgraph adapter, migrations, `storage status` | `core` |
| `lsp`, `platform-setup` | Language servers and the platform's LSP/hooks config | `workspace`, `core` / `cli`, `lsp`, `index` |
| `audit` | Per-session log | `core` |
| `index`, `graph`, `semantic-layer` | Parsing, graph, annotations and search | `storage`, … |
| `operations`, `data`, `execution-flow`, `flow-memory`, `incidents`, `dev-support`, `agent-prompts` | Operational tools, flows, incident memory, developer tools and prompts | as in the capability map |

Build order: `workspace → cli, core → contexts, storage, lsp → audit → graph → semantic-layer, operations, data → execution-flow → flow-memory, incidents → agent-prompts`. **Current slice:** `workspace` plus the CLI's `init` and `project` commands ([plan](../../tasks/plan.md)).

## 7. Quality attributes

| Attribute | How it is met |
|---|---|
| **Security** | No network listener; sockets `0600` in a `0700` directory; logs owner-only; secrets never exposed outside connection setup; production/staging/testing strictly read-only; free-form Cypher confined by an anchor guard to the namespace and view; no LLM call without config **and** flag. |
| **Safety of people's work** | Commands change nothing on failure; `INIT-MEMORY.md` is never overwritten; person-written annotations are never overwritten or deleted by agents; worktrees keep project checkouts untouched; project files are changed only by `developer-support` edit tools. |
| **Local-first** | One storage per person on their machine; no shared server; flows and memory stay local because `.workbench/` is git-ignored. |
| **Performance** | Incremental re-parse only; one query tool; the watcher batches bursts. Targets per module are in their specs. |
| **Testability** | Engine has no I/O, so use cases are tested with fake ports; adapters share a test suite against a fake and the real system; the CLI is tested by running the built binary. |
| **Operability** | Server and watcher are managed by `start | stop | status | logs`; `context check` and `storage status` diagnose before use. |

## 8. Risks and open points

| Risk | Mitigation |
|---|---|
| Memgraph must be running; default mode will not start without it | Clear start-up error with the `docker run` hint; `storage status` |
| Workspace spans several repos and branches | Per-view graphs, per-project groups, worktrees per context |
| Specs not yet written: `index`, `graph`, `semantic-layer`, `lsp`, `platform-setup`, `dev-support`, `operations`, `data`, `audit` and later modules | Built in order; each reviewed before its build starts |
| Dependencies not yet in `Cargo.toml` (`notify`, `reqwest`) and `deny.toml` allows only MIT, Apache-2.0, Unicode-3.0 | Added by the task that first uses them, with a reviewed licence exception if needed |

## 9. Related documents

Specs: [`docs/spec`](../spec/README.md). Plan for the current slice: [`tasks/plan.md`](../../tasks/plan.md). Low-level design: [`docs/lld`](../lld/README.md). ADRs: [`docs/adr`](../adr/README.md).
