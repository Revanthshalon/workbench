# Spec: graph

Module `graph` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`workspace`](SPEC-workspace.md) (approved) and [`storage`](SPEC-storage.md) (draft). Used by `index` (which runs it incrementally), `semantic-layer` (annotations anchor to its node ids), `execution-flow`, `operations`, `dev-support`, and by `contexts` (views).

## Decisions already made elsewhere

1. **One graph per project, per view**, plus a derived **overview** of the connections between projects per view (`SPEC-storage.md`, "Partitions"). Re-indexing one project never touches another.
2. **Views:** the project's working tree (`working`) and one per context (`context:<name>`). Node ids carry the view (`SPEC-contexts.md`). In `operational-support` every graph tool sees the session's active context view; in `developer-support` it sees `working`.
3. **Tree-sitter** parses Python, TypeScript, HTML, CSS, SCSS, YAML, Dockerfile and Docker Compose.
4. **The overview is derived, not parsed**: from Compose files (each project's own, and the ones listed at the top of the workspace config, with the optional `services` map), Dockerfiles, shared-library APIs and links people add as annotations. A Compose service matches a project by its build context path. It is replaced as a unit when an input changes. `INIT-MEMORY.md` is not parsed.
5. **Tools** (all kind `observe` except `reindex`): `service_map` (with `focus=…` for blast radius), `contract`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, `graph_query`, `graph_schema`, plus `callers`, `callees`, `dependents`, `test_map`, `changed_symbols`, and `reindex` (`record`). Each takes a project scope; the default is all projects.
6. `graph_query` is free-form, read-only Cypher over a documented, versioned query schema; the guard, limits and `graph_schema` mechanics are specified in `SPEC-storage.md`. **This spec defines the schema's content.**
7. *When* and *how much* to parse (content hashes, `git diff` shortcut, lock, partial failures, `--full`) belongs to `SPEC-index.md`. This spec defines **what a parse produces** and the operations on `GraphStore`.
8. `read_file`, like every code read, reads from the view's checkout and only inside a registered project.

## Assumptions (correct me now)

1. **Node ids are stable and readable:** `<view>/<project>/<path>#<qualified.name>` for code nodes (for example `working/billing-api/app/invoices.py#InvoiceService.create`), `<view>/<project>/<path>` for files, `<view>/<project>` for projects. Two nodes with the same qualified name in one file (overloads, redefinitions) get `~2`, `~3` in source order. Moving code inside a file keeps its id; renaming or moving it to another file changes it, so annotations on it become *orphaned* (`semantic-layer`), never silently re-pointed.
2. **No type inference.** Tree-sitter sees syntax only. A call or reference is resolved by name through imports and scope, and each edge carries `confidence`: `resolved` (one candidate by import or scope), `inferred` (several candidates, or a method call matched by name on a known class), or `unresolved` (kept as a name, no target). The indexer does **not** call language servers in the first release (Open Question 1); `lsp_definition` and `lsp_references` give exact answers on demand.
3. **Cross-project links come from evidence the files state.** The overview records a link only with at least one piece of evidence (file, line, kind). Kinds in the first release: `compose` (service, `depends_on`, network, port, environment reference), `dockerfile` (`FROM` another project's image, `COPY --from`), `shared-lib` (a project imports a package another project in the workspace provides), `http` (see 4), `annotation` (a person's `semantic add --kind link`). No guessing from naming. Kafka topics and databases are a later addition (Open Question 2).
4. **HTTP contracts** are found by patterns on literal values: routes declared with FastAPI, Flask or Starlette decorators; calls with Angular `HttpClient` and Python `httpx` or `requests` whose URL is a string literal or a literal path appended to a configured base. A call matches a route when the method matches and the path matches with parameters as wildcards. A call with a non-literal URL is recorded as `unresolved` and never linked.
5. **Limits:** a file over 1 MB, a generated or minified file (by a header marker or an average line over 500 characters) and a binary file are recorded as `File` nodes with `skipped: true` and a reason, not parsed. A file with syntax errors is parsed as far as tree-sitter recovers and marked `partial: true`.
6. **What is a node.** Enough for navigation and annotation, not every token: projects, files, classes, functions, methods, module-level constants, imports, routes, Angular components, services and directives, template elements that carry a binding or component selector, style rules by selector, Dockerfile stages, Compose services, networks and volumes, and top-level YAML keys. Bodies are not stored; `code_context` re-reads the source by line range.
7. **Test files** are recognised by convention (`test_*.py`, `*_test.py`, `tests/`, `*.spec.ts`, `*.test.ts`). `test_map` uses `TESTS` edges built from calls and imports in test files to non-test nodes; it is best-effort for the same reason as Assumption 2.
8. The parser version is a number kept with each indexed file. Bumping it makes the next run re-parse everything it applies to.
9. Tree-sitter grammars are new dependencies (one crate per language) and need approval.

## Objective

Give people and agents one queryable picture of the code across all projects (what exists, what calls what, and how services connect), cheap to keep fresh and honest about how sure it is.

The module is done when:
- each supported file kind parses into the nodes and edges below, deterministically, with ids that survive unrelated edits,
- a project's partition can be written, read and deleted alone, and the overview is rebuilt from its inputs as a unit,
- the tools return answers from the graph with `confidence` and `evidence`, scoped to the session's view and the requested projects,
- `graph_schema` and the real data agree: every label, relationship and property the schema lists exists, and nothing else is exposed.

## The query schema (version 1)

**Nodes** (label: properties). Every node has `id`, `project`, `view`, `kind`, `name`, `path`, `line_start`, `line_end`.

| Label | Extra properties |
|---|---|
| `Project` | `language`, `dependency_manager` |
| `File` | `file_kind` (see `SPEC-lsp.md`), `hash`, `size`, `skipped`, `partial`, `parser_version` |
| `Class`, `Function`, `Method`, `Constant` | `qualified_name`, `signature`, `decorators`, `docstring` (first 300 characters), `is_test` |
| `Import` | `module`, `names` |
| `Route` | `http_method`, `path_pattern`, `framework` |
| `Component`, `Service`, `Directive` | `selector`, `template`, `style_urls` (Angular) |
| `Element`, `StyleRule` | `tag` or `selector`, `bindings` |
| `Stage`, `Instruction` | Dockerfile: `base_image`, `command` |
| `ComposeService`, `Network`, `Volume` | `image`, `build_context`, `ports`, `depends_on`, `environment_keys` (names only, never values) |
| `YamlKey` | `key_path`, `value_kind` (scalar values are not stored) |
| `Unresolved` | `name` — the target of an edge that could not be resolved |

**Relationships** (all carry `confidence` where marked *): `CONTAINS` (project → file → symbols, parent → child), `IMPORTS`*, `CALLS`*, `EXTENDS`*, `REFERENCES`*, `DECORATED_BY`, `TESTS`*, `BUILDS` (Compose service → Dockerfile or project), `USES_IMAGE`, `DEPENDS_ON`, `HANDLES` (route → function), `CALLS_HTTP`* (function → route, across projects).

**Overview** relationships live in a separate partition per view and carry `evidence` (a list of `{file, line, kind}`): `LINKS_TO` (project → project, with `kinds`) and the `CALLS_HTTP`, `DEPENDS_ON`, `USES_IMAGE` edges between nodes of different projects.

Environment values, secrets in YAML or Compose, and file contents beyond a short docstring are never stored in the graph. `environment_keys` holds names only.

The schema is versioned (`schema_version: 1`). Adding a label or property is a compatible change; removing or renaming one bumps the version and needs a migration (`SPEC-storage.md`).

## The tools

| Tool | Input (besides `projects`, default all) | Answer |
|---|---|---|
| `service_map` | `focus` (project or node id, optional), `depth` (default 2) | The projects and their links with kinds and evidence; with `focus`, what is reachable from it (the blast radius), nearest first |
| `contract` | `project` or `route` | The routes a project exposes and the calls it makes, each with its match on the other side or `unresolved` |
| `find_symbol` | `name` (exact, or contains with `partial: true`), `kind` (optional) | Matching nodes: id, kind, project, path, line, signature |
| `code_context` | `node` (id) or `path` + `symbol` | The node, its source text (read from the checkout by line range, capped at 200 lines), its parents, direct callers and callees, annotations, tests |
| `trace_flow` | `from` (node id), `to` (optional), `max_depth` (default 6) | Call paths downwards from `from`, or the shortest paths to `to`, with `confidence` on each hop |
| `callers`, `callees` | `node`, `depth` (default 1, max 5) | The nodes that call it, or that it calls |
| `dependents` | `node` (symbol, file or route) | What depends on it across projects: importers, callers, HTTP clients, Compose dependants |
| `test_map` | `node` or `path` | Test nodes that exercise it, by `TESTS` edges |
| `changed_symbols` | `files` (list) or `git_diff` (a ref range) | The nodes whose lines the change touches, grouped by project |
| `read_file` | `project`, `path`, `from_line`, `to_line` | The text, capped at 256 KB, read from the view's checkout; a path outside the project, a symlink out of it, `.git/` and `.workbench/` are refused |
| `graph_query`, `graph_schema` | Cypher + parameters, or none | As in `SPEC-storage.md` |
| `reindex` (`record`) | `projects`, `full` | The result of the incremental run in `SPEC-index.md` |

Every list answer is limited (default 50, at most 500) and says when it was cut. Every answer states the view it came from and, where the index is behind the files on disk, how many files are pending.

## The overview

Derived by `graph` from its inputs and replaced as one unit per view whenever one of them changes:

```text
inputs:  each project's Compose file, the workspace-level Compose files, each project's Dockerfiles,
         the import graphs of the project partitions, HTTP contracts, link annotations
  → Compose services → projects (build context path, else the `services` map)
  → Dockerfile FROM/COPY --from → projects
  → shared libraries: a project's declared package ↔ other projects' imports of it
  → route ↔ call matches
  → merge: one LINKS_TO per project pair with all kinds and every piece of evidence
  → replace the view's overview partition
```

A Compose service that matches no project becomes an external node (name and image) so the picture shows it. A person's `link` annotation whose node no longer exists is shown as orphaned and dropped from the overview.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `thiserror`.
- New dependencies, needing approval: `tree-sitter` and one grammar per language (Python, TypeScript, HTML, CSS, SCSS, YAML, Dockerfile; Compose is YAML with its own reading rules), and a YAML parser for Compose and workspace-level inputs if the existing one is not enough.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Parse:   cargo run -p workbench -- index --project billing-api --full
Status:  cargo run -p workbench -- storage status
```

## Project Structure

```
lib/engine/src/graph/
├── model.rs       # NodeId, Node, Edge, Confidence, Evidence, PartitionData, ViewId
├── schema.rs      # the query schema (labels, relationships, properties) and its version
├── parse/         # parser trait + one module per file kind: python, typescript, html, css, scss, yaml, dockerfile, compose
├── resolve.rs     # name resolution through imports and scope, Confidence
├── contract.rs    # HTTP routes and calls, matching
├── overview.rs    # deriving the overview from Compose, Dockerfiles, shared libs, contracts, link annotations
├── query.rs       # service_map, find_symbol, callers, callees, dependents, trace_flow, test_map, changed_symbols
└── ports.rs       # SourceParser (tree-sitter behind a port), view checkout reader

lib/engine/src/storage/ports.rs   # GraphStore (operations below)

lib/adapters/src/graph/
├── treesitter.rs  # the SourceParser adapter, one grammar per kind
└── checkout.rs    # reading files of a view (working tree or worktree)

services/cli/src/mcp/modules/graph.rs   # the tools
```

**`GraphStore` operations** (added to the port in `SPEC-storage.md`): `write_partition(view, project, data)`, `read_partition_index(view, project)` (file hashes and parser versions only), `delete_partition`, `replace_overview(view, data)`, `read_overview(view)`, `node(id)`, `neighbours(id, relation, direction, depth, limit)`, `find(name, kind, projects, limit)`, `nodes_in_lines(file, from, to)`, `query` (storage spec), `schema_version`. Each is covered by the shared suite.

## Code Style

```rust
/// How sure the graph is about an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Resolved,
    Inferred,
    Unresolved,
}
```

Parsers are pure functions from source text to `PartitionData` fragments; they take no I/O. Follow existing conventions: doc comments on every public item, no `unwrap` or `panic`, ports in the engine.

## Testing Strategy

- **Parser fixtures:** per file kind, a small source file and the exact nodes and edges expected (golden files). Includes: syntax errors (partial, no panic), a skipped large file, overloads (`~2`), decorators, relative imports, Angular component with template and styles, Dockerfile multi-stage, Compose `depends_on`.
- **Id stability:** an unrelated edit above a function leaves its id unchanged; a rename changes it.
- **Resolution:** each `Confidence` level has a case; an ambiguous call is `inferred`, not `resolved`.
- **Contracts:** route/call matching by method and path with parameters; a non-literal URL stays `unresolved`.
- **Overview:** each input kind produces a link with evidence; a service with no matching project becomes an external node; changing one input replaces the overview and leaves every project partition unchanged.
- **Secrets:** Compose `environment` values and YAML scalars never appear in the stored graph (a fixture with a fake password).
- **Query tools:** fake `GraphStore`; scope narrowing; limits and truncation notice; view isolation (a session in `context:production` never sees `working` nodes).
- **Schema agreement:** `graph_schema` lists exactly the labels and relationships the parsers can produce (a test over the parser output).
- **`read_file`:** outside-project, symlink escape, `.git/`, `.workbench/` refused; line range and size cap.
- Every row of the tools table has a test; the `GraphStore` operations run in the shared suite against the fake and Memgraph.

## Boundaries

- **Always:** parse deterministically; give every edge a `confidence` and every cross-project link evidence; keep partitions independent; read code only from the view's checkout inside a registered project; cap list answers and say when cut.
- **Ask first:** adding a grammar or parser dependency; calling language servers during indexing; recognising new frameworks or contract kinds (Kafka, databases); changing the query schema in a way that renames or removes something.
- **Never:** store environment values, YAML scalar values or secrets in the graph; write to a project's files; guess a cross-project link without evidence; let a tool see another view than the session's; use the graph store from a tool other than through the port.

## Success Criteria

1. Every supported file kind parses into the nodes and edges above; a file with syntax errors is stored partially and never aborts a run.
2. Node ids are stable under unrelated edits and carry the view.
3. Each project's partition is written, read and deleted alone; the overview is derived from its inputs and replaced as a unit.
4. Every edge has a `confidence`; every cross-project link lists its evidence.
5. HTTP routes and calls with literal paths are matched across projects; non-literal ones stay unresolved.
6. The tools in the table return the described answers, scoped to the session's view and requested projects, with limits.
7. `graph_schema` agrees with the data, and no secret, environment value or YAML scalar is stored.
8. `read_file` refuses everything outside the project and caps its output.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Resolve calls with the language servers during indexing?** Exact, but slow and it needs servers installed and healthy. Proposal: no in the first release; `lsp_*` tools answer exactly on demand.
2. **Which contract kinds in the first release?** Proposal: HTTP only; Kafka topics (producers/consumers by literal topic name) and database tables follow as separate additions.
3. **Rename handling:** a renamed symbol gets a new id, so its annotations become orphaned. Proposal: keep that (honest, `prune` cleans up) rather than guessing renames.
4. **Limits:** 1 MB per file, 200 lines for `code_context`, 256 KB for `read_file`. Right sizes?
