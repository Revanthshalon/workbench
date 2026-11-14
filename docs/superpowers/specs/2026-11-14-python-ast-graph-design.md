# Python AST → code graph (step 1)

## Intent

`workbench index` should turn each project's code into a graph that is later
stored in Memgraph. The workspace is the root, projects hang off it, and the code
hangs off each project. Projects can connect to each other; the first kind of
cross-project connection is a code import (project A imports a package project B
provides). Service calls (HTTP between projects) come later and are out of scope.

This first step builds the language-neutral graph model and a real Python parser
that fills it. Nothing is written to Memgraph yet.

## Goals

- A graph model that every parser produces and every store consumes.
- Parse Python files with tree-sitter into that model: modules, classes,
  functions and methods, their containment, and their imports and base classes.
- Keep imports and base classes as **unresolved references**, so a later link step
  can resolve them within a project and across projects.
- Node ids that are unique across the workspace and stable across runs while the
  code they name keeps its name, so "re-linked" and stale annotations can work later.
- Files with syntax errors still yield the parts that parse (the existing "partial"
  outcome).

## Non-goals (this step)

- Writing to Memgraph, the workspace link step, resolving references.
- Call edges (`Calls` references), variables, attributes, decorators, type hints.
- TypeScript parsing (TypeScript projects keep the placeholder text check).
- Service-level connections, Compose-based edges.
- Changing the `index` summary columns.

## Anti-goals

- Storing the full syntax tree (every name, operator and literal). It is too large
  and has no stable identity.
- Requiring a Python interpreter on the machine.
- Failing a whole file because one function has a syntax error.

## Approaches considered

| Approach | Trade-off | Decision |
|---|---|---|
| tree-sitter-python | Pure Rust, tolerates syntax errors (partial files), but a concrete syntax tree we walk ourselves | **Chosen** |
| Python's `ast` via the project's interpreter | Exact semantics, but needs a working interpreter per project, a process per run, and fails whole files on syntax errors | Rejected |
| rustpython-parser 0.4 | Typed AST in Rust, but stops at the first syntax error; the crate looks stale | Rejected |

## The graph model

Language neutral, in a new module `src/graph/`.

```
FileGraph  { path, hash, module: NodeId, nodes: [Node], edges: [Edge], references: [Reference] }
Node       { id: NodeId, kind: NodeKind, name, span: Span }
NodeKind   = Module | Class | Function | Method
Span       { start_line, end_line }                  // 1-based, inclusive
Edge       { from: NodeId, to: NodeId, kind: EdgeKind }   // both ends in the same file
EdgeKind   = Contains
Reference  { from: NodeId, target: String, kind: ReferenceKind, line }
ReferenceKind = Imports | Inherits
```

The workspace and project nodes are not part of a `FileGraph`: they come from the
config, and the store creates them. A file's module node is linked to its project
by the store.

### Node ids

`<project-id>:<qualified name>`, for example
`billing-api:billing.invoices.Invoice.total`.

- **Module name** comes from the file path relative to the project:
  - drop the extension; `pkg/__init__.py` is the module `pkg`;
  - path separators become dots: `billing/invoices.py` → `billing.invoices`;
  - a leading `src/` directory is dropped when it is only a folder (the "src
    layout", no `src/__init__.py`): `src/billing/invoices.py` → `billing.invoices`.
    When `src/` is itself a package (it has an `__init__.py` or `__init__.pyi`), it
    is part of the name, matching how the code imports it:
    `src/billing/invoices.py` → `src.billing.invoices`. *(Changed after the first
    real run: a project importing `src.config` got ids without `src`, so imports
    could never match their modules.)*
- **Classes, functions, methods**: the module name, then the enclosing definitions,
  then the name: `billing.invoices.Invoice.total`.
- **Nested functions** are kept and named through their parent:
  `billing.jobs.run.retry`.
- If a name is defined twice in the same scope (for example, a function redefined
  under `if TYPE_CHECKING:`), the later definition gets the suffix `#2`, `#3`, ...,
  so ids stay unique.

### What becomes a node

- One `Module` node per file.
- `Class` for every `class` statement, at any depth.
- `Method` for a `def` directly inside a class body; `Function` for any other `def`.
  `async def` counts the same. Decorated definitions are included.
- Definitions inside `if`, `try` and `with` blocks count as part of the enclosing scope.

### Edges

- `Contains` from each node to the definitions directly inside it: module → class,
  class → method, function → nested function.

### References

Recorded from the node whose scope they appear in. `target` is a dotted name,
resolved as far as the file alone allows:

- `import a.b` → `Imports a.b`; `import a.b as c` → `Imports a.b`.
- `from a.b import c` → `Imports a.b.c`, one reference per imported name.
- `from a.b import *` → `Imports a.b`.
- Relative imports resolve against the file's own module:
  in `pkg.sub.mod`, `from .x import y` → `pkg.sub.x.y`; `from .. import y` → `pkg.y`.
  For a package's `__init__.py`, `.` is the package itself.
- `class C(Base)` → `Inherits <base>`. When `Base` was imported in the file, the
  target is the imported dotted name (`from models import Base` makes it
  `models.Base`); when it is defined in the same file, it is that node's qualified
  name; otherwise it is the name as written (`Exception`, `abc.ABC`).

## Fit with `workbench index`

- `Parser::parse` returns the graph:
  `Parsed(FileGraph)`, `Partial(FileGraph, details)` or `Failed(details)`.
- Python projects use the new `PythonParser`. TypeScript projects keep the text
  check, which returns a `FileGraph` with just the module node.
- A tree-sitter parse with error nodes is `Partial`: the graph contains everything
  outside the broken regions, and the log gets the line numbers of the errors.
- `ProjectChanges.upserted` carries `FileGraph`s instead of path and hash pairs.
  The Memgraph placeholder still writes nothing.
- The parser receives the project id (ids include it), the file's path relative to
  the project, and its contents. `index` computes the content hash, as today, and
  sets it on the graph.
- Unchanged files are not parsed, as today; the store keeps what it has for them.
- A module node's span is the whole file.

## Constraints

- New dependencies: `tree-sitter` 0.27.2 and `tree-sitter-python` 0.25.0. The
  grammar is C code compiled at build time, so builds need a C compiler (present on
  macOS with the Xcode tools and on the Ubuntu CI image).
- Clippy rules in `Cargo.toml` apply (`unwrap_used`, `panic` denied).
- Parsing must not change project files and must not need network access.

## Testing

Unit tests on source snippets, asserting the exact nodes, edges and references:
classes with methods, nested functions, `async def`, decorated definitions,
definitions inside `if`, each import form including relative and star imports,
inheritance from imported, local and builtin bases, duplicate names, `__init__.py`
and `src/` module naming, and a file with a syntax error producing a partial graph.
An `index` test checks that a Python project's files are parsed through the new
parser.

## Left to the builder

- The internal walk over tree-sitter nodes and how errors are collected.
- Exact names of internal helpers and modules beyond `src/graph/`.
