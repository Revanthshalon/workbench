# Spec: storage

Module `storage` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`core`](SPEC-core.md) (approved) and reads the `storage` block of the workspace config ([`workspace`](SPEC-workspace.md), approved). Used by `graph`, `semantic-layer`, `flow-memory`, `incidents`, the indexing runs (`workbench index`, `workbench watch`, MCP `reindex`) and `workbench storage status`.

## Decisions already made elsewhere

1. Workbench has **one storage: Memgraph**, running on the person's own machine. It is configured by the `storage` block of the **workspace config** (host, port, optional username, `password_env`, `namespace`), so `developer-support` and the CLI can use it without the server config. The defaults are host `localhost` and port `7687`. There is no other backend and no choice of backend.
2. The password is never stored: `storage.password_env` names an environment variable. The value is a `secrecy` secret, read only when a connection is opened.
3. `graph`, `semantic-layer`, `flow-memory` and `incidents` depend on engine ports, not on Memgraph. The ports are defined in the engine; the one adapter lives in `lib/adapters`. The only place Memgraph's language shows is the `graph_query` tool, which takes Cypher.
4. The AST graph and the semantic annotations live here. The audit record does **not**: it is the per-session log file (`SPEC-core.md`). Annotations written by people and flows are also mirrored to files by their own modules.
5. The graph is split into **views**: a project's working tree, and one per context branch (`SPEC-contexts.md`).
6. Several processes use the same storage at the same time: CLI commands, the background MCP server and the background watcher. The Memgraph server handles concurrent access; all indexing runs also share one lock so only one indexing run writes at a time.

## Assumptions (correct me now)

1. *(Confirmed)* **Writes and the engine's own reads use typed operations; agents get a free-form read-only query.** Each port offers the typed operations its consumer needs (upsert the nodes of a file, look up a symbol, walk callers to a depth). On top of that, the MCP `graph_query` tool takes a **free-form Cypher query**. The query always runs read-only, over a documented query schema (see "Free-form graph queries").
2. **Writes are whole batches.** A write takes a complete value (for example everything parsed from a set of files in one view) and applies it atomically. No transaction object crosses the port, so no Memgraph detail leaks out.
3. **Migrations are forward-only and automatic.** Every module registers its own migrations; they run when a process opens the storage. A database written by a newer `workbench` than the one opening it is refused with a clear message, never changed.
4. *(Confirmed)* **One storage per person.** There is no shared server: Memgraph runs on the person's own machine. A `storage.namespace` keeps several of that person's workspaces apart on one local Memgraph.
5. `workbench init` writes a unique `namespace` into the new workspace's `storage` block (the folder name plus four random hexadecimal characters), so two workspaces never share data by accident. A config with no `namespace` uses the namespace `default`, which is only safe for a single workspace.
6. *(Confirmed)* Search is a case-insensitive contains-scan with a result limit: `search_code` and annotation search do not rely on a full-text engine. That is acceptable for the first release, and using Memgraph's own text indexes is on the roadmap.
7. Data is sensitive (code structure, annotations, incident text), so Memgraph should listen only on the person's own machine.
8. *(Confirmed)* **Memgraph is required.** Workbench does not install, start or stop it: a person runs it, for example with `docker run -p 7687:7687 memgraph/memgraph`. Because `developer-support` is the default mode and needs storage, it will not start until Memgraph answers.
9. For `graph_query`, **the guard is the enforcement**: it rejects every write before the query reaches Memgraph. The query also runs in a read-mode transaction, but that is a second layer only, because it is not confirmed that Memgraph rejects writes in a read-mode transaction. To be confirmed when the adapter is built, and covered by the tests either way.

## Objective

Give every module that needs to remember something one place to put it, with safe access from several processes and with no module depending on Memgraph directly.

The module is done when:
- every port operation works against a running Memgraph, proved by one shared test suite that also runs against an in-memory fake used by the other modules' unit tests,
- the CLI, the MCP server and the watcher can use the same storage at once without corrupting it or losing writes,
- a database from a newer version is refused, and an older one is migrated,
- `workbench storage status` shows what is stored and whether Memgraph answers.

### What is stored

| Owner | Data | Defined in |
|---|---|---|
| `graph` | One graph per project and view (nodes, edges, content hashes and the parser version for each indexed file, so unchanged files are skipped), plus an overview of the connections between projects per view | `SPEC-graph.md` |
| `semantic-layer` | Annotations anchored to node ids (current, stale or orphaned) and remembered knowledge | `SPEC-semantic-layer.md` |
| `flow-memory` | Confirmed execution flows and the combined orchestration flow | `SPEC-flow-memory.md` |
| `incidents` | Incident reports and their fields | `SPEC-incidents.md` |

Each owner defines its own model and operations. `storage` defines the machinery they all share, below.

### The ports

| Port | Operations come from | Needed by |
|---|---|---|
| `Storage` | this spec: `open(config)`, `status()`, `migrate()` | every process |
| `GraphStore` | `SPEC-graph.md`, plus `query` (the free-form read-only Cypher query, this spec) | graph, indexing runs, execution-flow |
| `KnowledgeStore` | `SPEC-semantic-layer.md` | semantic-layer |
| `SearchIndex` | `SPEC-semantic-layer.md` (contains-scan search over annotations and code) | semantic-layer |
| `FlowStore` | `SPEC-flow-memory.md` | flow-memory |
| `IncidentStore` | `SPEC-incidents.md` | incidents |

**Rule:** every operation added to a port needs a test in the shared suite, which runs against both the in-memory fake and Memgraph.

### Opening the storage

```text
read the storage block of the workspace config (defaults: localhost:7687, namespace "default")
  → connect to Memgraph      fail: "cannot reach memgraph at <host>:<port>: <reason>. Start Memgraph, for example 'docker run -p 7687:7687 memgraph/memgraph'"
  → read each module's schema version
  → if the database is newer than this workbench      fail: "database schema vN is newer than this workbench supports (vM). Upgrade workbench."
  → apply pending migrations in order
  → ready
```

Opening storage is part of the start-up of both MCP modes (`SPEC-core.md`, "build adapters from the config"): if Memgraph cannot be reached the server does not start, and says why. CLI commands that need storage (`index`, `watch`, `semantic …`, `context sync`) fail the same way.

### Memgraph

- Reached over the Bolt protocol at `storage.host` and `storage.port`, with `storage.username` if one is set and the password from `password_env`.
- All of this workspace's data carries the `namespace` on every node and relationship, so several workspaces can share one local Memgraph without touching each other. Every statement filters on it and the indexes lead with it.
- Every statement is parameterised; nothing is built by joining strings.
- Uniqueness constraints and indexes on node ids and common lookups are created by the migrations.
- A write batch runs in one Memgraph transaction, so it is atomic.
- Concurrent access from several processes is handled by the server. When two transactions conflict, the adapter retries a bounded number of times, then reports `Conflict`. The shared index lock (`.workbench/run/index.lock`, a file lock) still keeps indexing runs from interleaving.
- Search: annotation and code search use a case-insensitive contains-scan with a result limit (Assumption 6).

### Migrations

- Every module registers `Migration { module, version, steps }`, where `steps` are Cypher statements. Versions are per module and only go up.
- The schema versions are stored in the database itself. A module's pending steps run one transaction per step, and the version is recorded only after the step succeeds.
- Two processes opening at the same time cannot both migrate: the second waits for the first, then sees nothing to do.
- There are no down-migrations in the first release.

### Partitions: one graph per project, plus an overview

- **A graph per project.** The graph is stored per view and project: each project's nodes, edges, content hashes and parser version form their own partition. A partition can be written, read, rebuilt or deleted on its own without touching any other project, so re-indexing one project never disturbs the rest. A partition is the `project` property on its nodes, under the workspace namespace.
- **An overview of the connections between projects.** Links *between* projects belong to no project's partition. They live in a separate **overview** partition per view, which is derived, not parsed: `graph` computes it from the Docker Compose files (each project's own file, and any file listed in the workspace config that spans projects), the Dockerfiles and the shared-library APIs of the projects, together with links people add as annotations, and replaces it as a unit whenever one of those inputs changes. A Compose service is matched to a project by its build context path, or by the optional `services` map in the workspace config (`SPEC-graph.md` defines the derived links). The workspace's `INIT-MEMORY.md` is context text for agents and is not parsed into links.
- **What `GraphStore` must offer for this:** write, read and delete one project partition; replace and read a view's overview partition; edges in the overview point at node ids of the project partitions; deleting a project removes its partition and the overview edges that point at it (they are recomputed).
- **Views.** The partitions exist once per checkout: the project's working tree and each context branch (`context:<name>`, `SPEC-contexts.md`). Storage must be able to delete a whole view, with every partition and its overview, as a unit; a later `context clean` uses this.

### Free-form graph queries

`graph_query` lets an agent ask anything the graph can answer, in Cypher. Because the language is free-form, **read-only enforcement and a documented schema** are what make it safe and usable.

**The query schema.** The node labels, relationship types and property names an agent queries are a documented, versioned surface. `SPEC-graph.md` defines its content. `graph_schema` returns that schema (kinds, properties, relationship types) with a few example Cypher queries and the schema version, so an agent knows what it may write.

**Read-only.**
1. **The guard.** Before a query reaches Memgraph it is tokenised (so strings and comments cannot hide a write). Only read clauses are allowed (`MATCH`, `OPTIONAL MATCH`, `WHERE`, `WITH`, `RETURN`, `UNWIND`, `ORDER BY`, `SKIP`, `LIMIT`) and a list of safe functions. Everything else is rejected, including `CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `DROP`, `FOREACH`, `LOAD CSV`, procedure calls (`CALL`) and more than one statement. The guard is what enforces read-only (Assumption 9).
2. **A read-mode transaction.** The query runs in a read-mode transaction as a second layer.
3. **Limits.** A row limit (200 by default, 5 000 at most), a time limit (10 seconds by default) and a limit on the size of the result.

**Results.** Columns and rows, with each value a string, number, boolean, null, list or map. Errors name the problem (a rejected clause is named). The query text, the row count and the time are written to the session log in operational mode.

**Parameters.** Queries take named parameters passed separately, never joined into the text.

### Errors

One error type: `Unavailable` (cannot connect), `Conflict` (a transaction conflict that did not clear after the retries), `SchemaTooNew`, `Corrupt`, `NotFound` and `Backend` (anything else). Messages name the host and port, never contain a secret, and have every resolved secret removed by exact match before they are shown or logged.

### `workbench storage status`

A read-only command (surface in `SPEC-cli.md`) that prints where Memgraph is (`host:port` and the namespace), whether it answers and how fast, each module's schema version, and per view the counts of indexed files, nodes, edges and annotations. It changes nothing and runs the migrations only to *read* versions, never to apply them.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `secrecy`, `thiserror` (already workspace dependencies).
- A Bolt client for Memgraph (for example `neo4rs`) is a new dependency and needs approval.
- A way to run Memgraph locally and in CI (a `docker compose` file or a Makefile target, and a Memgraph service container in the CI job) is new and needs approval. The repository's `.gitignore` already reserves `/docker/` for a Memgraph bind mount.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace                       # the engine and the in-memory fake
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Memgraph: cargo test --workspace -- --ignored          # needs a local Memgraph (docker run -p 7687:7687 memgraph/memgraph); CI runs it with a service container
Status:   cargo run -p workbench -- storage status
```

## Project Structure

```
lib/engine/src/storage/
├── mod.rs         # re-exports
├── ports.rs       # Storage, GraphStore, KnowledgeStore, SearchIndex, FlowStore, IncidentStore (operations added by the consumer specs)
├── error.rs       # StoreError
├── migration.rs   # Migration, the registry, the ordering and "database too new" rule
├── status.rs      # StoreStatus, ModuleVersion, ViewCounts
└── fake.rs        # an in-memory implementation of the ports, for the other modules' unit tests

lib/adapters/src/storage/
├── mod.rs         # open(&StorageConfig) -> Storage
├── memgraph/      # Bolt connection, namespace handling, the Cypher guard, migrations, one file per port
└── suite.rs       # the shared test suite, run against the fake and against Memgraph

services/cli/src/commands/storage.rs   # `workbench storage status`
```

The engine has no I/O: it holds the ports, the error, the migration rules and the in-memory fake.

## Code Style

A migration is data the registry can order and check:

```rust
/// One forward-only schema step of one module.
pub struct Migration {
    pub module: &'static str,
    pub version: u32,
    pub steps: &'static [&'static str],
}
```

Follow the existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine, the adapter in `lib/adapters`.

## Testing Strategy

- **Engine unit tests:** migrations apply in order per module, never twice, and a database newer than the registry is refused; the error type never carries a secret.
- **Shared suite** (`lib/adapters/src/storage/suite.rs`): one set of tests for every operation of every port (round trips, batch atomicity, view deletion, ordering, stale and orphaned states). It runs against the in-memory fake in the normal test run, so the fake can be trusted by other modules, and against Memgraph with `--ignored` (CI runs it with a service container). An operation without a test in the suite is not accepted.
- **Memgraph tests** (`--ignored`, against a running server): namespaces keep two workspaces apart; every statement is parameterised; two processes writing at once both succeed or one gets `Conflict` after the retries; an unreachable server fails with the hint message; the password never appears in output.
- **Free-form query tests:** every write attempt is rejected by the guard and the database is unchanged afterwards (`CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `DROP`, `FOREACH`, `CALL`, `LOAD CSV`, a write hidden in a comment or a string, a second statement); the tests pass whether or not Memgraph itself rejects writes in a read-mode transaction; a valid read returns the expected rows; the row, time and size limits apply; parameters are never joined into the text; `graph_schema` lists exactly the query surface and nothing internal.
- **Partition tests:** writing, rebuilding or deleting one project's partition leaves the others unchanged; replacing the overview leaves every project partition unchanged; deleting a project removes the overview edges that pointed at it.
- **Index lock tests:** two indexing runs never write at once; the second waits.
- **Migration tests:** an old fixture database migrates; a too-new one is refused unchanged; two simultaneous opens migrate once.
- **Integration tests** (`services/cli/tests/`): `workbench storage status` on a fresh workspace and on one with indexed data, and with Memgraph unreachable (the message names the host and port).
- Every row of the port table and every rule under "Memgraph" and "Migrations" has a test.

## Boundaries

- **Always:** run free-form queries through the guard, read-only, with limits, over the documented query schema; apply writes as whole atomic batches; parameterise every statement; filter every statement by the namespace; refuse a database newer than this version; remove resolved secrets from error text.
- **Ask first:** adding a dependency (the Bolt client) or a compose file for Memgraph; widening what the query schema exposes or the Cypher guard allows; down-migrations; any change to the stored data that cannot be re-created by indexing again; adding another storage backend.
- **Never:** let `graph_query` write, even with a clever query; run a free-form query on the connection used for writes; build a statement by joining strings; let a consumer module use Memgraph directly; store a secret; migrate a database that is newer than this version; delete or change data outside this workspace's namespace.

## Success Criteria

1. With no `storage` block, a workspace uses Memgraph at `localhost:7687` in the namespace `default`; `workbench init` writes a unique namespace into the new config.
2. The shared suite passes against the in-memory fake in the normal run and against Memgraph (with `--ignored`, and in CI), with identical results.
3. The CLI, the background server and the watcher use one Memgraph at the same time without corruption; conflicting writes retry, then report `Conflict`.
4. Two indexing runs never write at once; the second waits for the first.
5. A database from a newer version is refused with the version numbers and is left unchanged; an older one is migrated, once, even if two processes open it together.
6. Opening storage fails with a message naming the host and port, and the hint to start Memgraph, when Memgraph cannot be reached, and the MCP server does not start.
7. The Memgraph password is read from the named environment variable only when connecting, and appears in no output or log.
8. Each project's graph is stored, rebuilt and deleted on its own without touching another project's; the overview of cross-project connections is a separate partition per view, replaced as a unit; deleting a project removes its partition and the overview edges pointing at it; deleting a view removes exactly that view's data; nothing outside the workspace's namespace is read or changed.
9. `graph_query` runs a free-form read-only Cypher query over the documented schema; no write attempt of any kind changes the database; limits apply; `graph_schema` returns the schema and example queries.
10. `workbench storage status` prints host, port, namespace, answer time, schema versions and per-view counts, and changes nothing.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench storage reset` and a backup or export command.
- Down-migrations.
- Use Memgraph's own text indexes for `search_code` and annotation search, instead of the contains-scan.

## Open Questions

None.
