# Spec: storage

Module `storage` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`core`](SPEC-core.md) (approved) and reads the `storage` block of the workspace config ([`workspace`](SPEC-workspace.md), approved). Used by `graph`, `semantic-layer`, `flow-memory`, `incidents`, the indexing runs (`workbench index`, `workbench watch`, MCP `reindex`) and `workbench storage status`.

## Decisions already made elsewhere

1. The backend is **SQLite or Memgraph**, chosen by the `storage` block of the **workspace config**, so `developer-support` and the CLI can use it without the server config. The default is SQLite at `.workbench/data/workbench.db`.
2. A Memgraph password is never stored: `storage.memgraph.password_env` names an environment variable. The value is a `secrecy` secret, read only when a connection is opened.
3. `graph`, `semantic-layer`, `flow-memory` and `incidents` do not know which backend is in use. The engine defines ports; the adapters live in `lib/adapters`.
4. The AST graph and the semantic annotations live here. The audit record does **not**: it is the per-session log file (`SPEC-core.md`). Annotations written by people and flows are also mirrored to files by their own modules.
5. The graph is split into **views**: a project's working tree, and one per context branch (`SPEC-contexts.md`).
6. Several processes use the same storage at the same time: CLI commands, the background MCP server and the background watcher. All indexing runs share one lock so only one indexing run writes at a time.

## Assumptions (correct me now)

1. *(Confirmed)* **Writes and the engine's own reads use typed operations; agents get a free-form read-only query.** Each port offers the typed operations its consumer needs (upsert the nodes of a file, look up a symbol, walk callers to a depth), and both adapters implement every one with the same meaning. On top of that, the MCP `graph_query` tool takes a **free-form query in the backend's own language**: SQL on SQLite, Cypher on Memgraph. The query always runs read-only, over a documented query schema (see "Free-form graph queries"). Making the language the same on both backends is on the roadmap.
2. **Writes are whole batches.** A write takes a complete value (for example everything parsed from a set of files in one view) and applies it atomically. No transaction object crosses the port, so no backend detail leaks out.
3. **Migrations are forward-only and automatic.** Every module registers its own migrations; they run when a process opens the storage. A database written by a newer `workbench` than the one opening it is refused with a clear message, never changed.
4. *(Confirmed)* **One storage per person.** There is no shared server: SQLite is a file on the person's machine, and Memgraph runs on the person's own machine. An optional `storage.memgraph.namespace` (default `default`) keeps several of that person's workspaces apart on one local Memgraph. This adds one optional field to the `storage` block of `SPEC-workspace.md`, which is amended to match.
5. *(Confirmed)* Search on Memgraph is more limited than SQLite's FTS5, so `search_code` and annotation search there fall back to a case-insensitive contains-scan with a result limit. That is acceptable for the first release.
6. Data is sensitive (code structure, annotations, incident text). The SQLite directory is created `0700` and the file `0600`.

## Objective

Give every module that needs to remember something one place to put it, whichever backend a person chose, with safe access from several processes and with no module depending on a specific database.

The module is done when:
- the same operations give the same results on SQLite and on Memgraph (proved by one shared test suite run against both),
- the CLI, the MCP server and the watcher can use the same storage at once without corrupting it or losing writes,
- a database from a newer version is refused, and an older one is migrated,
- `workbench storage status` shows what is stored and whether the backend answers.

### What is stored

| Owner | Data | Defined in |
|---|---|---|
| `graph` | One graph per project and view (nodes, edges, content hashes and the parser version for each indexed file, so unchanged files are skipped), plus an overview of the connections between projects per view | `SPEC-graph.md` |
| `semantic-layer` | Annotations anchored to node ids (current, stale or orphaned), remembered knowledge, and the search index over them | `SPEC-semantic-layer.md` |
| `flow-memory` | Confirmed execution flows and the combined orchestration flow | `SPEC-flow-memory.md` |
| `incidents` | Incident reports and their fields | `SPEC-incidents.md` |

Each owner defines its own model and operations. `storage` defines the machinery they all share, below.

### The ports

| Port | Operations come from | Needed by |
|---|---|---|
| `Storage` | this spec: `open(config)`, `status()`, `migrate()` | every process |
| `GraphStore` | `SPEC-graph.md`, plus `query` (the free-form read-only query, this spec) | graph, indexing runs, execution-flow |
| `KnowledgeStore` | `SPEC-semantic-layer.md` | semantic-layer |
| `SearchIndex` | `SPEC-semantic-layer.md` (search over annotations and code) | semantic-layer |
| `FlowStore` | `SPEC-flow-memory.md` | flow-memory |
| `IncidentStore` | `SPEC-incidents.md` | incidents |

**Rule:** a consumer spec may add a *typed* operation to its port only if both adapters can implement it with the same meaning and the shared test suite covers it. The free-form `query` is the one dialect-specific operation.

### Opening the storage

```text
read the storage block of the workspace config (default: sqlite at .workbench/data/workbench.db)
  → open the backend            fail: "cannot reach memgraph at <host>:<port>: <reason>"
  → read each module's schema version
  → if the database is newer than this workbench      fail: "database schema vN is newer than this workbench supports (vM). Upgrade workbench."
  → apply pending migrations in order
  → ready
```

Opening storage is part of the start-up of both MCP modes (`SPEC-core.md`, "build adapters from the config"): if the backend cannot be reached the server does not start, and says why. CLI commands that need storage (`index`, `watch`, `semantic …`, `context sync`) fail the same way.

### SQLite

- One database file, opened in **WAL mode** with a 5 second busy timeout, foreign keys on and `synchronous=NORMAL`.
- Readers use their own connections and see a consistent snapshot; writes go through one writer at a time (serialised inside a process, and by SQLite across processes).
- Every write batch is one transaction. A write that cannot get the lock within the busy timeout fails with `Locked`, which callers may retry.
- Indexing runs also hold the shared index lock (`.workbench/run/index.lock`, a file lock), so two runs never interleave; short writes such as an annotation do not need it and just use the busy timeout.
- Full-text search uses SQLite's FTS5.
- The file lives in `.workbench/data/` (git-ignored with the rest of `.workbench/`), directory `0700`, file `0600`.

### Memgraph

- Reached over the Bolt protocol at `storage.memgraph.host`, `port` and `username`, with the password from `password_env`.
- All of this workspace's data carries the `namespace`, so several workspaces can share a server without touching each other.
- Every statement is parameterised; nothing is built by joining strings.
- Uniqueness constraints and indexes on node ids and common lookups are created by the migrations.
- A write batch runs in one Memgraph transaction, so it is atomic too.
- Concurrency across processes is handled by the server. The shared index lock still applies to indexing runs.
- Search: no full-text engine is assumed; annotation and code search fall back to a case-insensitive contains-scan with a result limit (Assumption 5).

### Migrations

- Every module registers `Migration { module, version, sqlite, memgraph }` steps. Versions are per module and only go up.
- The schema versions are stored in the backend itself. Applying a module's pending steps is atomic where the backend allows it (SQLite: one transaction; Memgraph: one transaction per step, recorded after it succeeds).
- Two processes opening at the same time cannot both migrate: the second waits for the first, then sees nothing to do.
- There are no down-migrations in the first release.

### Partitions: one graph per project, plus an overview

- **A graph per project.** The graph is stored per view and project: each project's nodes, edges, content hashes and parser version form their own partition. A partition can be written, read, rebuilt or deleted on its own without touching any other project, so re-indexing one project never disturbs the rest.
- **An overview of the connections between projects.** Links *between* projects belong to no project's partition. They live in a separate **overview** partition per view, which is derived, not parsed: `graph` computes it from the Docker Compose files (each project's own file, and any file listed in the workspace config that spans projects), the Dockerfiles and the shared-library APIs of the projects, together with links people add as annotations, and replaces it as a unit whenever one of those inputs changes. A Compose service is matched to a project by its build context path, or by the optional `services` map in the workspace config (`SPEC-graph.md` defines the derived links). The workspace's `INIT-MEMORY.md` is context text for agents and is not parsed into links.
- **What `GraphStore` must offer for this:** write, read and delete one project partition; replace and read a view's overview partition; edges in the overview point at node ids of the project partitions; deleting a project removes its partition and the overview edges that point at it (they are recomputed).
- **Views.** The partitions exist once per checkout: the project's working tree and each context branch (`context:<name>`, `SPEC-contexts.md`). Storage must be able to delete a whole view, with every partition and its overview, as a unit; a later `context clean` uses this.

### Free-form graph queries

`graph_query` lets an agent ask anything the graph can answer, in the language of the configured backend. Because the language is free-form, **read-only enforcement and a documented schema** are what make it safe and usable.

**The query schema.** The tables or graph labels an agent queries are a documented, versioned surface, separate from the adapters' internal storage. `SPEC-graph.md` defines its content; this spec requires:
- *SQLite:* read-only SQL **views** (for example `q_nodes` and `q_edges`) with stable column names, and nothing else visible to the query connection.
- *Memgraph:* node labels, relationship types and property names that match the same documented model.
- `graph_schema` returns the dialect (`sql` or `cypher`), the schema (kinds, properties, relationship types, with a few example queries for that dialect) and its version, so an agent knows what it may write.

**Read-only, in two layers.**
1. *SQLite:* queries run on a **separate connection opened read-only** with `query_only` on, an authoriser that allows only `SELECT` and reads from the query views, a single statement per call, a time limit (10 seconds by default) and a row limit.
2. *Memgraph:* queries run in a read transaction after a **guard** that tokenises the Cypher (so strings and comments cannot hide a write) and allows only read clauses (`MATCH`, `OPTIONAL MATCH`, `WHERE`, `WITH`, `RETURN`, `UNWIND`, `ORDER BY`, `SKIP`, `LIMIT`) and a list of safe functions, and rejects everything else: `CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `DROP`, `FOREACH`, `LOAD CSV`, procedure calls (`CALL`) and more than one statement.
3. Both: a row limit (200 by default, 5 000 at most), a time limit, and a limit on the size of the result.

**Results.** Columns and rows, with each value a string, number, boolean, null, list or map. Errors name the dialect and the problem (a rejected clause is named). The query text, the row count and the time are written to the session log in operational mode.

**Parameters.** Queries take named parameters passed separately, never joined into the text.

### Errors

One error type for every backend: `Unavailable` (cannot connect), `Locked` (busy timeout), `SchemaTooNew`, `Corrupt`, `NotFound` and `Backend` (anything else). Messages name the backend and the location, never contain a secret, and have every resolved secret removed by exact match before they are shown or logged.

### `workbench storage status`

A read-only command (surface in `SPEC-cli.md`) that prints the backend, where it is (a path or `host:port`), whether it answers and how fast, each module's schema version, and per view the counts of indexed files, nodes, edges and annotations, plus the file size for SQLite. It changes nothing and runs the migrations only to *read* versions, never to apply them.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `secrecy`, `thiserror` (already workspace dependencies).
- New dependencies, each needing approval: a SQLite binding with the bundled library and FTS5 (for example `rusqlite` with `bundled`, run on blocking threads) and a Bolt client for Memgraph (for example `neo4rs`).
- A way to run Memgraph locally for tests (a `docker compose` file or a Makefile target) is a new file and needs approval.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace                       # SQLite adapter and the engine
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Memgraph: cargo test --workspace -- --ignored          # needs a local Memgraph; run by hand
Status:   cargo run -p workbench -- storage status
```

## Project Structure

```
lib/engine/src/storage/
├── mod.rs         # re-exports
├── ports.rs       # Storage, GraphStore, KnowledgeStore, SearchIndex, FlowStore, IncidentStore (operations added by the consumer specs)
├── error.rs       # StoreError
├── migration.rs   # Migration, the registry, the ordering and "database too new" rule
└── status.rs      # StoreStatus, ModuleVersion, ViewCounts

lib/adapters/src/storage/
├── mod.rs         # open(&StorageConfig) -> Box<dyn Storage>: picks the adapter
├── sqlite/        # connection setup (WAL, timeout), migrations, one file per port
├── memgraph/      # Bolt connection, namespace handling, migrations, one file per port
└── conformance.rs # the shared test suite, run against both adapters

services/cli/src/commands/storage.rs   # `workbench storage status`
```

The engine has no I/O: it only holds the ports, the error and the migration rules.

## Code Style

A migration is data the registry can order and check, and each backend gets its own statement:

```rust
/// One forward-only schema step of one module.
pub struct Migration {
    pub module: &'static str,
    pub version: u32,
    pub sqlite: &'static str,
    pub memgraph: &'static [&'static str],
}
```

Follow the existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **Engine unit tests:** migrations apply in order per module, never twice, and a database newer than the registry is refused; the error type never carries a secret.
- **Conformance suite** (`lib/adapters/src/storage/conformance.rs`): one set of tests for every operation of every port (round trips, batch atomicity, view deletion, ordering, stale and orphaned states), written once and run against **each** adapter. SQLite runs in the normal test run; Memgraph runs with `--ignored` against a local server. A new port operation without a conformance test is not accepted.
- **SQLite tests:** WAL is on; two processes writing at once both succeed or one gets `Locked`; a reader sees a consistent snapshot while a writer is mid-batch; the file is `0600` in a `0700` directory.
- **Free-form query tests** (both backends): every write attempt is rejected and the database is unchanged afterwards (`INSERT`, `UPDATE`, `DELETE`, `DROP`, `ATTACH`, `PRAGMA` writes, `CREATE`, `MERGE`, `SET`, `CALL`, `LOAD CSV`, a write hidden in a comment or a string, a second statement); a valid read returns the same logical answer on both backends for the same data; the row, time and size limits apply; parameters are never joined into the text; `graph_schema` lists exactly the query surface and nothing internal.
- **Partition tests:** writing, rebuilding or deleting one project's partition leaves the others byte-for-byte unchanged; replacing the overview leaves every project partition unchanged; deleting a project removes the overview edges that pointed at it.
- **Index lock tests:** two indexing runs never write at once; the second waits.
- **Migration tests:** an old fixture database migrates; a too-new one is refused unchanged; two simultaneous opens migrate once.
- **Integration tests** (`services/cli/tests/`): `workbench storage status` on a fresh workspace and on one with indexed data, in both a working and a failing backend case (Memgraph unreachable names the host).
- Every row of the port table and every rule under "SQLite", "Memgraph" and "Migrations" has a test.

## Boundaries

- **Always:** run free-form queries read-only, with limits, over the documented query schema; apply writes as whole atomic batches; parameterise every Memgraph statement; keep the SQLite file `0600` in a `0700` directory; refuse a database newer than this version; remove resolved secrets from backend error text.
- **Ask first:** adding a dependency (the SQLite binding, the Bolt client) or a compose file for Memgraph; adding a typed port operation that only one backend can implement; widening what the query schema exposes or the Cypher guard allows; down-migrations; any change to the stored data that cannot be re-created by indexing again.
- **Never:** let `graph_query` write, even with a clever query; run a free-form query on the connection used for writes; build a Memgraph statement by joining strings; let a consumer module name a backend; store a secret; migrate a database that is newer than this version; delete or change data outside this workspace's namespace.

## Success Criteria

1. With no `storage` block, a fresh workspace uses SQLite at `.workbench/data/workbench.db`, created `0600` in a `0700` directory.
2. The conformance suite passes on SQLite in the normal run and on Memgraph with `--ignored`, with identical results.
3. The CLI, the background server and the watcher use one SQLite file at the same time without corruption; a busy write retries or reports `Locked`.
4. Two indexing runs never write at once; the second waits for the first.
5. A database from a newer version is refused with the version numbers and is left unchanged; an older one is migrated, once, even if two processes open it together.
6. Opening storage fails with a message naming the backend and its location when Memgraph cannot be reached, and the MCP server does not start.
7. The Memgraph password is read from the named environment variable only when connecting, and appears in no output or log.
8. Each project's graph is stored, rebuilt and deleted on its own without touching another project's; the overview of cross-project connections is a separate partition per view, replaced as a unit; deleting a project removes its partition and the overview edges pointing at it; deleting a view removes exactly that view's data.
9. `graph_query` runs a free-form read-only query in the backend's language over the documented schema; no write attempt of any kind changes the database; limits apply; `graph_schema` names the dialect and the schema.
10. `workbench storage status` prints the backend, location, answer time, schema versions and per-view counts, and changes nothing.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench storage reset` and a backup or export command.
- Down-migrations.
- A backend-neutral graph query language, so `graph_query` reads the same on SQLite and Memgraph.

## Open Questions

None. The questions from the earlier drafts are answered and folded into the sections above.
