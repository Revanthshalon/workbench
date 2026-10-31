# Spec: storage

Module `storage` from the [capability map](capability-map.md). Status: **approved.**

Depends on [`core`](SPEC-core.md) (approved) and reads the `storage` block of the workspace config ([`workspace`](SPEC-workspace.md), approved). Used by `graph`, `semantic-layer`, `flow-memory`, `incidents`, the indexing runs (`workbench index`, `workbench watch`, MCP `reindex`) and `workbench storage status`.

## Decisions

1. Workbench has **one storage: Memgraph**, running on the person's own machine. It is configured by the `storage` block of the **workspace config** (host, port, optional username, `password_env`, `namespace`), so `developer-support` and the CLI can use it without the server config. The defaults are host `localhost` and port `7687`. There is no other backend and no choice of backend.
2. The password is never stored: `storage.password_env` names an environment variable. The value is a `secrecy` secret, read only when a connection is opened.
3. `graph`, `semantic-layer`, `flow-memory` and `incidents` depend on engine ports, not on Memgraph. The ports are defined in the engine; the one adapter lives in `lib/adapters`. The only place Memgraph's language shows is the `graph_query` tool, which takes Cypher.
4. The AST graph and the semantic annotations live here. The audit record does **not**: it is the per-session log file (`SPEC-core.md`). **Annotations written by people are stored here and also mirrored to `.workbench/memory/<project-id>/annotations.yml`** (the project's folder, created when the project is added), so `workbench index` can replay them into a Memgraph that was emptied or rebuilt, and customisations survive re-parsing the AST and LLM-drafted annotations (`SPEC-semantic-layer.md`). Agent-written annotations are not mirrored, because they can be regenerated. Flows are likewise mirrored to files by their own module.
5. The graph is split into **views**: a project's working tree, and one per context branch (`SPEC-contexts.md`).
6. Several processes use the same storage at the same time: CLI commands, the background MCP server and the background watcher. The Memgraph server handles concurrent access; all indexing runs also share one lock so only one indexing run writes at a time.
7. **Writes and the engine's own reads use typed operations; agents get a free-form read-only query.** Each port offers the typed operations its consumer needs (upsert the nodes of a file, look up a symbol, walk callers to a depth). On top of that, the MCP `graph_query` tool takes a **free-form Cypher query**. The query always runs read-only, over a documented query schema (see "Free-form graph queries").
8. **Writes are whole batches.** A write takes a complete value (for example everything parsed from a set of files in one view) and applies it atomically. No transaction object crosses the port, so no Memgraph detail leaks out.
9. **Migrations are forward-only and automatic.** Every module registers its own migrations; they run when a process opens the storage. A database written by a newer `workbench` than the one opening it is refused with a clear message, never changed.
10. **One storage per person.** There is no shared server: Memgraph runs on the person's own machine. A `storage.namespace` keeps several of that person's workspaces apart on one local Memgraph.
11. `workbench init` writes the whole `storage` block into the new workspace's config: `host: localhost`, `port: 7687` and a unique `namespace` (the folder name plus four random hexadecimal characters), so the settings are visible and editable and two workspaces never share data by accident. A config that lacks the block, or a field of it, uses the same defaults (`localhost`, `7687`), and a missing `namespace` is `default`, which is only safe for a single workspace.
12. Search is a case-insensitive contains-scan with a result limit: `search_code` and annotation search do not rely on a full-text engine. That is acceptable for the first release, and using Memgraph's own text indexes is on the roadmap.
13. Data is sensitive (code structure, annotations, incident text), so Memgraph should listen only on the person's own machine.
14. **Memgraph is required.** Workbench does not install, start or stop it: a person runs it, for example with `docker run -p 7687:7687 memgraph/memgraph`. Because `developer-support` is the default mode and needs storage, it will not start until Memgraph answers.
15. For `graph_query`, **the guard is the enforcement**: it rejects every write before the query reaches Memgraph. The query also runs in a read-mode transaction, but that is a second layer only, because it is not confirmed that Memgraph rejects writes in a read-mode transaction. To be confirmed when the adapter is built, and covered by the tests either way.
16. **One graph per view, so queries cross projects but never views; the guard keeps them inside the workspace and the session's view.** Each view is a complete graph of the workspace; contexts differ by branch, not by project list. Every node carries `namespace`, `view` and `project`. A free-form query must be anchored at a node constrained to `$namespace` and `$view` (both bound by the adapter and reserved), which is enough because relationships never join different namespaces or views. "Partition" means a logical grouping by `project` and `view`, never graph partitioning in the algorithmic sense.

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
| `graph` | One graph per view (the working trees and each context), each with a `Project` node per project and its nodes grouped logically by `project` (nodes, edges, content hashes and the parser version for each indexed file, so unchanged files are skipped), plus an overview of the connections between projects per view, including people's custom links and custom nodes | `SPEC-graph.md` |
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
- All of this workspace's data carries the `namespace` on every node and relationship, so several workspaces can share one local Memgraph without touching each other. Every typed statement filters on it and the indexes lead with it; free-form `graph_query` statements are confined by the guard's anchor rule (see "Free-form graph queries").
- Every statement is parameterised; nothing is built by joining strings.
- Uniqueness constraints and indexes on node ids and common lookups are created by the migrations.
- A write batch runs in one Memgraph transaction, so it is atomic.
- Concurrent access from several processes is handled by the server. When two transactions conflict, the adapter retries a bounded number of times, then reports `Conflict`. The shared index lock (`.workbench/run/index.lock`, a file lock) still keeps indexing runs from interleaving.
- Search: annotation and code search use a case-insensitive contains-scan with a result limit (Decision 12).

### Migrations

- Every module registers `Migration { module, version, steps }`, where `steps` are Cypher statements. Versions are per module and only go up.
- The schema versions are stored in the database itself. A module's pending steps run one transaction per step, and the version is recorded only after the step succeeds.
- Two processes opening at the same time cannot both migrate: the second waits for the first, then sees nothing to do.
- There are no down-migrations in the first release.

### One database per workspace, one graph per view: projects as nodes, connected through service boundaries

- **One database per workspace (the namespace), and in it one graph per view.** A **view** is one checkout of the workspace's projects: their working trees (`working`, what `developer-support` sees) or one context's branches (`context:production`, `context:staging`, `context:testing`, `SPEC-contexts.md`). **Each view is its own complete graph**: a `Project` node for every project, their code nodes, the services their Compose files declare, their connections, and the custom links and nodes people added for that context. Within one view the graph is a single graph, so one Cypher query follows relationships from a node of one project into a node of another. Different views are **different graphs**: they share no nodes or relationships, no relationship ever joins two views, and a query sees exactly one view. Nothing is split into separate databases or shards per project.
- **Every node carries three properties:** `namespace` (the workspace), `view` (the graph it belongs to: `working` or `context:<name>`) and `project` (the owning project's id).
- **A partition is a logical grouping, not graph partitioning.** In these specs a *partition* means the nodes that share a `project` and a `view`. It is a way to name the unit a write, a rebuild or a delete works on. It is **not** graph partitioning in the algorithmic sense: no graph is cut, sharded, balanced or stored apart, and it creates no boundary that a query has to respect. Re-indexing one project rewrites that project's group and leaves every other group alone.
- **A node per project, in every view.** Each project is a `Project` node in each view, and its files, classes, functions and other nodes hang under it. The group also holds the project's content hashes and parser version. **Contexts differ by branch, not by project list:** every context uses the workspace's projects and repositories, each at the context's own branch (`SPEC-contexts.md`), so the code, the Compose files and therefore the services and connections in a context's graph are those of its branch.
- **Projects connect through their service boundaries.** A connection between projects is a relationship between nodes of different projects in the same graph: a route and the call to it, a Compose service that builds a project, a project that imports another's library. Each one is also summarised as a `LINKS_TO` relationship between the two `Project` nodes. They belong to no project's group. They come from three sources:
  1. **Docker Compose files** (each project's own file, at the view's own checkout, and any file listed in the workspace config that spans projects), the **Dockerfiles** and the **shared-library APIs** of the projects. A Compose service is matched to a project by its build context path, or by the optional `services` map in the workspace config.
  2. **Custom annotations**: a person adds a `link` annotation between two nodes (`workbench semantic add --kind link … --context <NAME>`) to state a connection the files do not show. **A link belongs to the contexts it names**: every link must name at least one (`working`, `production`, `staging` or `testing`), because a connection that holds in one environment need not hold in another.
  3. **Custom nodes**: a `link` annotation may point at a named node that no project's code defines, such as a shared database, a queue or a third-party API (`--to-custom <NAME>`). The first link to a name creates the **custom node** (label `Custom`) in each view the link names; later links reuse it. A custom node exists in a view only while a link in that view points at it.
  The annotation, with the contexts it names, is the source of truth for sources 2 and 3; the nodes and relationships it produces are written into those views' graphs when they are indexed: it is mirrored to `annotations.yml` and replayed with the other annotations (`SPEC-semantic-layer.md`). `SPEC-graph.md` defines the derived links and their evidence. The workspace's `INIT-MEMORY.md` is context text for agents and is not parsed into links.
- **The overview is those cross-project relationships and custom nodes.** They are tagged `layer: 'overview'`, derived rather than parsed, and replaced as a unit per view (the old ones are deleted and the new ones created in one transaction) whenever one of their inputs changes.
- **What `GraphStore` must offer for this:** write, read and delete one project's group; replace and read a view's overview; overview relationships point at node ids in the project groups; deleting a project removes its group and the overview relationships that point at it (they are recomputed). A project removed from the workspace config keeps its data until an indexing run that covers every project deletes it (`SPEC-index.md`); `workbench project remove` itself does not touch the storage.
- **Views are separate graphs told apart by the `view` property.** The same projects appear once per view. Storage must be able to delete a whole view (its groups, overview and custom nodes) as a unit; a later `context clean` uses this. Annotations of the kinds `purpose`, `role`, `domain-term` and `tag` are not tied to a view: they attach to a code address and show wherever that code exists; only `link` annotations name their contexts.

### Free-form graph queries

`graph_query` lets an agent ask anything the graph can answer, in Cypher. Because the language is free-form, **read-only enforcement and a documented schema** are what make it safe and usable.

**The query schema.** The node labels, relationship types and property names an agent queries are a documented, versioned surface. `SPEC-graph.md` defines its content. `graph_schema` returns that schema (kinds, properties, relationship types) with a few example Cypher queries and the schema version, so an agent knows what it may write.

**Read-only.**
1. **The guard.** Before a query reaches Memgraph it is tokenised (so strings and comments cannot hide a write). Only read clauses are allowed (`MATCH`, `OPTIONAL MATCH`, `WHERE`, `WITH`, `RETURN`, `UNWIND`, `ORDER BY`, `SKIP`, `LIMIT`) and a list of safe functions. Everything else is rejected, including `CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `DROP`, `FOREACH`, `LOAD CSV`, procedure calls (`CALL`) and more than one statement. The guard is what enforces read-only (Decision 15).
2. **A read-mode transaction.** The query runs in a read-mode transaction as a second layer.
3. **Limits.** A row limit (200 by default, 5 000 at most), a time limit (10 seconds by default) and a limit on the size of the result.

**Scope: queries span projects, never workspaces or views.** The adapter binds two reserved parameters, `$namespace` (this workspace) and `$view` (the session's view: `working` in `developer-support`, the active context's view in `operational-support`); a query cannot bind parameters with those names. **The guard requires an anchor:** every path pattern in every `MATCH` and `OPTIONAL MATCH` must contain a node constrained to `namespace = $namespace` and `view = $view`, either in its property map or as top-level `AND` conjuncts of the clause's `WHERE`. A query with no anchor, or with the anchor under `OR` or `NOT`, is rejected with the form to use:

```cypher
MATCH (c:Function {namespace: $namespace, view: $view, name: 'charge'})<-[:CALLS*1..3]-(caller)
RETURN caller.project, caller.qualified_name
```

Relationships never join nodes of different namespaces or views, so a path anchored at one node stays inside that namespace and view; within it the query reaches every project, and narrows with a `project` property when it wants one. `graph_schema` shows the anchor in every example.

**Results.** Columns and rows, with each value a string, number, boolean, null, list or map. Errors name the problem (a rejected clause is named). The query text, the row count and the time are written to the session log in operational mode.

**Parameters.** Queries take named parameters passed separately, never joined into the text. `namespace` and `view` are reserved (see Scope).

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
- **Free-form query tests:** every write attempt is rejected by the guard and the database is unchanged afterwards (`CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `DROP`, `FOREACH`, `CALL`, `LOAD CSV`, a write hidden in a comment or a string, a second statement); the tests pass whether or not Memgraph itself rejects writes in a read-mode transaction; a valid read returns the expected rows; the row, time and size limits apply; parameters are never joined into the text; `graph_schema` lists exactly the query surface and nothing internal; **anchor tests:** a query with no `namespace`/`view` anchor, with the anchor under `OR` or `NOT`, or binding its own `$namespace` or `$view`, is rejected; an anchored multi-project query returns rows from several projects and none from another namespace or view (a fixture holds two namespaces and two views); **view isolation:** the same project indexed at two branches gives two separate sets of nodes with no relationship between them, and a link or custom node scoped to `production` is invisible in `staging` and `working`.
- **Grouping tests:** writing, rebuilding or deleting one project's group (its `project` and `view`) leaves the others unchanged, and one query still follows a relationship across two projects; replacing the overview leaves every project partition unchanged; deleting a project removes the overview edges that pointed at it.
- **Index lock tests:** two indexing runs never write at once; the second waits.
- **Migration tests:** an old fixture database migrates; a too-new one is refused unchanged; two simultaneous opens migrate once.
- **Integration tests** (`services/cli/tests/`): `workbench storage status` on a fresh workspace and on one with indexed data, and with Memgraph unreachable (the message names the host and port).
- Every row of the port table and every rule under "Memgraph" and "Migrations" has a test.

## Boundaries

- **Always:** run free-form queries through the guard, read-only, with limits, over the documented query schema; apply writes as whole atomic batches; parameterise every statement; filter every statement by the namespace; refuse a database newer than this version; remove resolved secrets from error text.
- **Ask first:** adding a dependency (the Bolt client) or a compose file for Memgraph; widening what the query schema exposes or the Cypher guard allows; down-migrations; any change to the stored data that cannot be re-created by indexing again; adding another storage backend.
- **Never:** let `graph_query` write, even with a clever query; run an unanchored free-form query; run a free-form query on the connection used for writes; build a statement by joining strings; let a consumer module use Memgraph directly; store a secret; migrate a database that is newer than this version; delete or change data outside this workspace's namespace.

## Success Criteria

1. `workbench init` writes a `storage` block with `host: localhost`, `port: 7687` and a unique namespace into the new config; with no `storage` block, or a field missing, a workspace uses Memgraph at `localhost:7687` in the namespace `default`.
2. The shared suite passes against the in-memory fake in the normal run and against Memgraph (with `--ignored`, and in CI), with identical results.
3. The CLI, the background server and the watcher use one Memgraph at the same time without corruption; conflicting writes retry, then report `Conflict`.
4. Two indexing runs never write at once; the second waits for the first.
5. A database from a newer version is refused with the version numbers and is left unchanged; an older one is migrated, once, even if two processes open it together.
6. Opening storage fails with a message naming the host and port, and the hint to start Memgraph, when Memgraph cannot be reached, and the MCP server does not start.
7. The Memgraph password is read from the named environment variable only when connecting, and appears in no output or log.
8. A workspace has one namespace and one graph per view; in each, a project's group of nodes is stored, rebuilt and deleted on its own without touching another project's, one query crosses projects, and no relationship or query crosses views; the overview of cross-project connections is a separate derived layer per view, replaced as a unit; deleting a project removes its partition and the overview edges pointing at it; deleting a view removes exactly that view's data; nothing outside the workspace's namespace is read or changed.
9. `graph_query` runs a free-form read-only Cypher query over the documented schema, anchored by the guard to the workspace and the session's view (`$namespace`, `$view`); no write attempt of any kind changes the database; limits apply; `graph_schema` returns the schema and example queries.
10. `workbench storage status` prints host, port, namespace, answer time, schema versions and per-view counts, and changes nothing.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench storage reset` and a backup or export command.
- Down-migrations.
- Use Memgraph's own text indexes for `search_code` and annotation search, instead of the contains-scan.

## Open Questions

None.
