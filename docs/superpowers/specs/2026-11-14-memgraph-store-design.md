# Storing the code graph in Memgraph (step 3)

## Intent

`workbench index` builds a code graph per file but stores nothing. This step writes
that graph to Memgraph, configured in the workspace config's `storage` block. Two
things come out of it:
- The future MCP server can query the workspace: assistants will ask questions such
  as "what inherits from `MainModel`?" and the server answers from Memgraph.
- `workbench` itself reads stored hashes back, so `unchanged` and `removed` become
  real and incremental runs work.

Before anything is stored, the last id hole is closed (deferred minor #1 of the
parser fixes): a name defined at module level must not take a submodule's id.

## Goals

1. Names at module level never share an id with another file's module, e.g.
   `settings` in `pkg/__init__.py` next to `pkg/settings.py`.
2. `index` writes each project's changed files to Memgraph and removes deleted
   files, in one transaction per project.
3. The stored graph holds:
   - the workspace, its projects and their files;
   - every code node, with kind, name, file path and lines;
   - `CONTAINS` edges;
   - every reference, unresolved, for the link step.
4. `unchanged` and `removed` come from what Memgraph holds.
5. A full run removes the data of projects no longer in the config, as
   `workbench project remove` already promises.
6. The Memgraph client is built by default.

## Non-goals

- The link step: resolving references to nodes. `re-linked` stays 0.
- Annotations: `stale` and `orphaned` stay 0.
- The MCP server itself.
- TypeScript parsing. TypeScript files store their module node only, as today.
- TLS to Memgraph, and Memgraph authentication beyond username and password.

## Anti-goals

- A half-written project after a failed write.
- Data from one workspace leaking into another sharing the same Memgraph.
- A run that silently stores nothing when Memgraph is unreachable.

## Approaches considered

**Writing**

| Approach | Trade-off | Decision |
|---|---|---|
| Replace per file: delete a changed or removed file's subgraph, then create the new one; batched; one transaction per project | Simple, always correct; unchanged files untouched; Memgraph's internal node ids change, but nothing uses them | **Chosen** |
| Upsert by id and diff deletions | Keeps internal identity nobody needs; an id diff per file | Rejected |
| Rebuild each project every run | Defeats incremental runs: unchanged files are not parsed | Rejected |

**Module-level names equal to a submodule**

| Approach | Trade-off | Decision |
|---|---|---|
| Suffix only the colliding name with `@<path>` (nested names follow) | One extra input for the parser; every other id unchanged | **Chosen** |
| Treat the whole file as a collision loser | Reuses the existing mechanism, but every id in the file changes | Rejected |

**Build:** `memgraph` becomes a default Cargo feature (chosen), rather than an opt-in
one that makes it easy to ship a binary that stores nothing.

## Design

### 1. Module-level names equal to another file's module

- **Input:** `SourceFile` gains `modules: Option<&HashSet<String>>`, the module names
  of every file in the project. `index` already computes them for collisions.
- **Rule:** when a class, function or variable at **module scope** gets a qualified
  name equal to a module name in that set (other than its own module), its qualified
  name gets `@<path>`, the defining file's path. For example
  `api:pkg.settings@pkg/__init__.py`, while `pkg/settings.py` keeps
  `api:pkg.settings`. This matches Python: `import pkg.settings` makes the submodule
  win.
- **Nesting:** names nested under a suffixed definition build on the suffixed name,
  e.g. `api:pkg.settings@pkg/__init__.py.load`, so they stay unique.
- **Binding:** the binding for base-class resolution uses the suffixed name.
- **Scope:** only module scope can collide. A deeper name builds on a module-scope
  name, and that one is suffixed first if it collides. Collisions between whole
  files (`pkg/x.py` and `pkg/x/__init__.py`) are still handled by the existing
  collision rules.

### 2. The stored graph

Every node carries `namespace` (the workspace's) and, below the workspace, `project`.
Every query filters on `namespace`.

| Label | Properties |
|---|---|
| `Workspace` | `namespace` |
| `Project` | `namespace`, `id`, `path`, `language`, `framework` (if set) |
| `File` | `namespace`, `project`, `path`, `hash` (16 lowercase hex digits) |
| `Code` plus one of `Module`, `Class`, `Function`, `Method`, `Variable` | `namespace`, `project`, `id`, `kind`, `name`, `path` (file), `start_line`, `end_line` |
| `Reference` | `namespace`, `project`, `path` (file), `target`, `line` |

| Relationship | From → to |
|---|---|
| `CONTAINS` | `Workspace` → `Project` → `File` → its module node; and every `Contains` edge of the file's graph |
| `IMPORTS`, `INHERITS` (property `line`) | the referencing code node → its own `Reference` node |

- **References** each get their own `Reference` node, not shared between referencing
  nodes, so deleting a file's subgraph removes them. The link step will later add
  edges from them, or replace them.
- **Hashes** are stored as hex text, because Memgraph integers are signed 64-bit and
  ours are unsigned.
- **Indexes:** `Code(id)`, `File(path)`, `Project(id)` and `Workspace(namespace)` are
  created when the store connects. Creating an index that exists must not fail.

### 3. Writing a project

`MemgraphStore` implements `Storage`:
- **`stored_hashes(project)`:** the `path` and `hash` of the project's `File` nodes.
- **`write_project(project, changes)`:** in **one transaction**:
  1. Merge the `Workspace` node, and the `Project` node with its properties and
     `CONTAINS` edge.
  2. For each removed path and each upserted path, delete the file's subgraph: the
     `File` node, every code node it contains (transitively), and their `Reference`
     nodes.
  3. Create the upserted files' `File` nodes, code nodes, `CONTAINS` edges, and
     `Reference` nodes with their relationships, batched with `UNWIND` lists.
  4. Commit. On any error, roll back. The project then shows `write failed` and keeps
     its previous data.
- **Project details:** `write_project` needs the project's path, language and
  framework, so the `Storage` trait's `write_project` takes the project's details
  (an `ExistingProject`) rather than just its id. This is a builder's choice, as long
  as the trait carries them.
- **Counts:** the returned `WriteResult` keeps `relinked`, `stale` and `orphaned` at 0.

### 4. Removed projects

- After a run that covered every project (no `--id`), the store deletes every
  `Project` in the namespace whose id is not in the config, with all its files, code
  nodes and references, in one transaction.
- **Output:** the summary gains a line naming them, such as
  `Removed indexed data of 1 project no longer in the config: old-api`.
- **On failure:** the run fails (exit 1) with a friendly message, and the details go
  to the log.
- **Interrupted runs:** a run interrupted before finishing skips this step.

### 5. Connecting

- **Connection settings** come from the `storage` block: `host`, `port`,
  `namespace`, and optionally `username` plus `password_env`, the *name* of the
  environment variable holding the password.
- **When:** `index` connects once, before taking the lock and parsing.
- **If the connection fails,** the run fails immediately (exit 1) with:
  ``Could not connect to Memgraph at <host>:<port>. Is it running? Details are in
  <log>.``
- **Required settings:**
  - A missing `namespace` fails with a message pointing at `storage.namespace`.
  - A `password_env` naming an unset variable fails the same way.
- **The summary's storage line** becomes `Storage: Memgraph at <host>:<port>,
  namespace <ns>`.

### 6. Build

- **Feature:** `memgraph` is in Cargo's `default` features.
- **Without it:** `--no-default-features` builds with the existing placeholder, and
  its storage line says `Storage: none (built without Memgraph support; nothing was
  written)`.

## Constraints

- **Client:** `rsmgclient` 4.0.0, which is synchronous, takes parameterised queries
  (lists and maps), and has manual transactions.
- **Build tools:** default builds need cmake and OpenSSL 3. On macOS the client looks
  only in Homebrew's `openssl@3`.
- **The existing project rules hold:**
  - ids as `<project-id>:<qualified name>`;
  - clippy denies `unwrap_used` and `panic`, in tests too;
  - tests in a `tests.rs` beside each module's `mod.rs`;
  - no Claude attribution in commits.
- **Integration tests against Memgraph are opt-in.** They run only when
  `WORKBENCH_TEST_MEMGRAPH` is set to `host:port`. Each test uses its own namespace
  and deletes it afterwards. Without the variable they skip, so the normal suite and
  CI need no Memgraph.
- **Development server:** the Memgraph 3.14.0 container on `127.0.0.1:7687`. It does
  not persist data.

## Testing

- **Unit:**
  - module-level names equal to a submodule, and nested names under them;
  - building the write batches from `ProjectChanges` (pure functions turning graphs
    into query parameters);
  - reading the `storage` settings, including username and `password_env`;
  - the storage line with and without the feature.
- **Integration (opt-in):**
  - write a project, then read the hashes back;
  - re-run with no changes (everything `unchanged`);
  - change and delete files (old nodes gone, new ones present);
  - a failed write rolls back;
  - removing a project from the config deletes its data on the next full run;
  - two namespaces do not see each other.
- **Real run:** `test-project` against the container; `index` twice (second run all
  `unchanged`); then a Cypher query for the classes inheriting `MainModel`.

## Left to the builder

- Query text and batching sizes.
- How the connection is held, for example a `RefCell` inside the store, given the
  trait's `&self`.
- Exact wording of error messages beyond those fixed above.
