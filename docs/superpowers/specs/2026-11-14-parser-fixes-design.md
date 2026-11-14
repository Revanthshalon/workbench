# Code graph parser fixes (step 2)

## Intent

The first real run (`test-project`, project `demo`) showed the code graph's shape is
right, but some ids and references would not line up in the coming link step. This
step fixes the six "Next steps" in
`docs/superpowers/plans/2026-11-14-python-ast-graph.md`, so the graph is settled
before anything is written to Memgraph. Memgraph itself is out of scope.

## Goals

1. Module ids are unique within a project.
2. Module-level variables are nodes, so imports such as
   `from src.config import settings` have a node to link to.
3. Base classes resolve to the binding in effect where the class is defined:
   - the last import or definition before it, in source order;
   - classes defined inside functions;
   - the `#2` id of a class defined twice.
4. Dotted bases are built from identifiers, so whitespace never reaches a target.

## Non-goals

- Writing to Memgraph, the link step, resolving references across files.
- Class attributes and local variables as nodes.
- Variables declared any other way than assignment (`for`, `with ... as`, `global`,
  walrus, `type X = ...`).
- TypeScript parsing.

## Anti-goals

- Making every common id less readable to handle a rare collision.
- A graph that differs depending on `--changed` (which files were re-parsed).

## Approaches considered

**Module id collisions**

| Approach | Trade-off | Decision |
|---|---|---|
| The winner keeps its dotted id; the others get a path suffix | Readable ids in the common case; collision handling needs the project's file list | **Chosen** |
| Path-based ids everywhere (`demo:src/config.py::Settings`) | Unique by construction, but less readable, and every id changes | Rejected |

**Variables**

| Approach | Trade-off | Decision |
|---|---|---|
| Module-level `Variable` nodes | Covers what imports point at, small graph | **Chosen** |
| Module and class-level variable nodes | More complete, many more nodes | Rejected for now |
| No new nodes; the link step falls back to the module | Nothing to annotate, imprecise | Rejected |

**Base-class resolution** (my call, explained below)

| Approach | Trade-off | Decision |
|---|---|---|
| Bindings tracked during the walk, per scope, in source order | Matches Python: a base is evaluated where the class is defined. Fixes goals 3a–3c together | **Chosen** |
| Keep the whole-file name map and patch each case | Three separate patches, and it stays wrong for out-of-order code | Rejected |

## Design

### 1. Module id collisions

- `index` computes the module id of **every** source file of the project before
  parsing, including with `--changed`, so ids never depend on which files are
  re-parsed. "Every source file" means what discovery already indexes: the same
  extensions and skipped directories. Files skipped for size or as symbolic links
  still count, since they are still the project's modules.
- **The grouping key** is the module name plus whether the file is a `.pyi` stub, so
  stubs only collide with stubs.
- **The winner** is the file Python, or the toolchain, would actually load, by these
  rules in order:
  1. a package file (`pkg/x/__init__.py`) over a plain module (`pkg/x.py`);
  2. in a plain src layout, a file under `src/` over one outside it (`src/x.py` over
     a root script `x.py`);
  3. the earlier extension in the language's extension list (`.py` before `.pyi`;
     `.ts` before `.tsx` before `.js`);
  4. the alphabetically first path.
- **Losers** get the suffix `@<path>` on every id in the file:
  `demo:pkg.x@pkg/x.py`, `demo:pkg.x.Foo@pkg/x.py`. For a losing stub, `@<path>`
  replaces `@pyi`, since the path already ends in `.pyi`. Order within an id:
  qualified name, any `#n`, then the suffix.
- **Logging:** each collision gets one `INFO` log line naming the module, the winner
  and each loser with its suffix.
- **Plumbing:** `SourceFile` gains `disambiguator: Option<&str>`, the loser's path.
  `FileGraph::module_only` and the Python parser append `@<disambiguator>` when it is
  set. This covers TypeScript's placeholder graphs too.

### 2. Module-level variables

- **New kind:** `NodeKind::Variable`.
- **Which statements count:** assignment statements in module scope, including
  inside module-level `if`, `try`, `with`, `for` and `while` blocks, but not inside
  functions or classes.
- **What becomes a variable:**
  - plain names on the left: `x = …`, `x: T = …`, `x: T`;
  - each name in tuple or list unpacking: `a, b = …`;
  - each name in a chain: `a = b = …`.

  Attribute and subscript targets (`obj.x = …`, `d[k] = …`) and augmented assignment
  (`x += …`) are not variables.
- **One node per name per module,** at its first assignment. The span is that
  statement's lines, and reassignments add nothing.
- **Definitions win the name.** If a class or function of the same name is defined
  anywhere in module scope, there is no variable node for it. This covers
  `f = decorate(f)`.
- **Id and edge:** `<project>:<module>.<name>`, with any stub or collision suffix, and
  a `Contains` edge from the module.

### 3. Base-class resolution through scope bindings

- **The walk keeps a stack of scopes.** Each scope has a binding map from local name
  to dotted name, filled in source order as the walk passes:
  - imports, using the bindings the parser already computes;
  - class definitions: the name maps to that class's qualified name, including its
    `#n`;
  - module-level variables: the name maps to the variable's qualified name. This
    resolves `Base = declarative_base()` followed by `class User(Base)`.

  A later binding of the same name replaces an earlier one.
- **A base's first segment** is looked up in this order:
  1. the scope the `class` statement is in;
  2. the enclosing function scopes, outwards. Class-body scopes further out are
     skipped, as in Python;
  3. the module scope.

  If nothing is found, the base stays as written. The rest of the dotted name is
  appended unchanged.
- **The whole-file name map built before the walk is removed.**
- **Known limit, accepted:** a class defined inside a function can see module-level
  names bound later in the file, since the function runs later. Its bases resolve
  only against bindings made before the function in source order.

### 4. Dotted bases from identifiers

- **The base's dotted name is built from the syntax tree, never the raw text:**
  - an `identifier` is its name;
  - an `attribute` is its `object`'s dotted name, then `.`, then its `attribute`;
  - a `subscript` uses its `value`, as today.
- **Any other base expression,** such as a call like `make_base()`, gives no
  reference, as today.

## Constraints

- The existing Global Constraints of the first plan still apply: ids as
  `<project-id>:<qualified name>`, 1-based lines, clippy denies `unwrap_used` and
  `panic`, tests in `tests.rs` beside each `mod.rs`, and no Claude attribution in
  commits.
- The walk must stay safe on deeply nested expressions; it does not descend into
  expressions.

## Testing

- Unit tests per rule: each collision precedence rule, and stub and TypeScript
  collisions; each variable form and each exclusion; source-order rebinding, nested
  bases, `#2` bases, variable bases, and spaced dotted bases.
- An `index` test checks that collisions are computed across all files with
  `--changed`.
- A re-run on a copy of `test-project` checks that the four former variable
  targets (`src.config.settings`, `src.database.database.METADATA`, and the two
  `*_table`s) now match nodes.

## Left to the builder

- Internal names, and how the scope stack is threaded through the walk.
- The exact wording of the collision log line.
