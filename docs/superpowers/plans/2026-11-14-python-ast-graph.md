# Python AST → Code Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse Python files with tree-sitter into a language-neutral code graph (modules, classes, functions, methods, containment, unresolved imports and base classes) that `workbench index` hands to storage.

**Architecture:** A new `src/graph/` module defines `FileGraph` and its parts. A `PythonParser` in the index command walks the tree-sitter syntax tree in two passes: first imports and module-level classes, to build a name map, then definitions and references. The index `Parser` trait returns `FileGraph`s, and `ProjectChanges` carries them to the `Storage` trait. The Memgraph placeholder still writes nothing.

**Tech Stack:** Rust 2024, `tree-sitter` 0.27.2, `tree-sitter-python` 0.25.0, clap, the existing index module.

**Spec:** `docs/superpowers/specs/2026-11-14-python-ast-graph-design.md`

## Global Constraints

- Dependencies: `tree-sitter = "0.27.2"` and `tree-sitter-python = "0.25.0"`. Builds need a C compiler.
- Node ids: `<project-id>:<qualified name>`, e.g. `billing-api:billing.invoices.Invoice.total`.
- Lines are 1-based and inclusive.
- Clippy with `-D warnings` must pass. `unwrap_used` and `panic` are denied, also in tests: return `Result` from tests instead.
- Tests live in a separate `tests.rs` beside each module's `mod.rs`, wired with `#[cfg(test)] mod tests;`.
- Parsing never writes to project files and never uses the network.
- Commit messages carry no Claude attribution lines or other trace of Claude (the user's standing instruction).

## Review Focus

1. **A `.py` and a `.pyi` for the same module** (`pkg/x.py` and `pkg/x.pyi`): their ids must not collide. A stub's node ids put `@pyi` after the qualified name, e.g. `api:pkg.x.Foo@pyi`. Tested in Task 2.
2. **A relative import that climbs above the top package** (`from ... import x` in `a.b`): no panic, and the target keeps the dots as written (`...x`). Tested in Task 3.
3. **An empty file or a file of only comments**: `Parsed`, with only the module node, span `1..=1`. Tested in Task 2.
4. **Non-ASCII names and CRLF line endings** (`def café():`, `\r\n`): names come out intact and line numbers stay correct. Tested in Task 2.
5. **A root-level `__init__.py` or `src/__init__.py`**: the module name is `__init__`, never empty. Tested in Task 1.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/graph/mod.rs` (create) | Graph types, `module_name`, `node_id` |
| `src/graph/tests.rs` (create) | Module naming and id tests |
| `src/main.rs` (modify) | `mod graph;` |
| `src/cli/commands/index/parser.rs` → `parser/mod.rs` (move) | `Parser` trait, `ParseOutcome`, `TextParser` |
| `src/cli/commands/index/parser/python/mod.rs` (create) | `PythonParser`: tree-sitter walk |
| `src/cli/commands/index/parser/python/tests.rs` (create) | Parser tests on source snippets |
| `src/cli/commands/index/storage.rs` (modify) | `ProjectChanges.upserted: Vec<FileGraph>`; drop `IndexedFile` |
| `src/cli/commands/index/mod.rs` (modify) | Per-language parser choice, set hash on graphs |
| `src/cli/commands/index/tests.rs` (modify) | Fakes updated, Python graph test |
| `README.md` (modify) | Index section: Python is parsed for real |

---

### Task 1: Graph model and module naming

**Files:**
- Create: `src/graph/mod.rs`, `src/graph/tests.rs`
- Modify: `src/main.rs` (add `mod graph;` beside `mod cli;`)

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Eq)]`, fields `pub`):
  - `pub type NodeId = String;`
  - `pub enum NodeKind { Module, Class, Function, Method }`
  - `pub struct Span { pub start_line: usize, pub end_line: usize }`
  - `pub struct Node { pub id: NodeId, pub kind: NodeKind, pub name: String, pub span: Span }`
  - `pub enum EdgeKind { Contains }`
  - `pub struct Edge { pub from: NodeId, pub to: NodeId, pub kind: EdgeKind }`
  - `pub enum ReferenceKind { Imports, Inherits }`
  - `pub struct Reference { pub from: NodeId, pub target: String, pub kind: ReferenceKind, pub line: usize }`
  - `pub struct FileGraph { pub path: String, pub hash: u64, pub module: NodeId, pub nodes: Vec<Node>, pub edges: Vec<Edge>, pub references: Vec<Reference> }`
  - `pub fn module_name(relative_path: &str) -> String`. Takes a `/`-separated path relative to the project.
  - `pub fn node_id(project: &str, qualified_name: &str) -> NodeId` returns `format!("{project}:{qualified_name}")`.
  - `impl FileGraph { pub fn module_only(project: &str, relative_path: &str, line_count: usize) -> Self }`. One `Module` node named by `module_name`, span `1..=max(line_count, 1)`, `hash: 0`, no edges or references.

- [ ] **Step 1: Write the failing tests** in `src/graph/tests.rs`

```rust
#[test]
fn module_names_follow_python_packages() {
    assert_eq!(module_name("billing/invoices.py"), "billing.invoices");
    assert_eq!(module_name("billing/__init__.py"), "billing");
    assert_eq!(module_name("src/billing/invoices.py"), "billing.invoices");
    assert_eq!(module_name("app.py"), "app");
    assert_eq!(module_name("pkg/types.pyi"), "pkg.types");
    assert_eq!(module_name("tools/src/x.py"), "tools.src.x"); // only a leading src/
}

#[test]
fn root_init_files_are_named_init() {
    assert_eq!(module_name("__init__.py"), "__init__");
    assert_eq!(module_name("src/__init__.py"), "__init__");
}

#[test]
fn ids_join_project_and_qualified_name() {
    assert_eq!(node_id("billing-api", "billing.invoices.Invoice.total"), "billing-api:billing.invoices.Invoice.total");
}

#[test]
fn module_only_graph_has_one_module_node() {
    let graph = FileGraph::module_only("web", "src/main.ts", 0);
    assert_eq!(graph.module, "web:main");
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].kind, NodeKind::Module);
    assert_eq!(graph.nodes[0].span, Span { start_line: 1, end_line: 1 });
    assert!(graph.edges.is_empty() && graph.references.is_empty());
}
```

`module_name` drops any final extension, so `main.ts` becomes `main`. TypeScript reuses it until it gets its own parser.

- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test graph::`. Expected: a compile error, because `module_name` and the other items are not defined.
- [ ] **Step 3: Implement the types and functions** in `src/graph/mod.rs`, with a `//!` module doc that says the model is language neutral.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test graph::`. Expected: 4 passed.
- [ ] **Step 5: Commit.** `git add src/graph src/main.rs && git commit -m "Add language-neutral code graph model"`

---

### Task 2: Python definitions → nodes and edges

**Files:**
- Modify: `Cargo.toml`. Add `tree-sitter = { version = "0.27.2" }` and `tree-sitter-python = { version = "0.25.0" }`.
- Move: `src/cli/commands/index/parser.rs` → `src/cli/commands/index/parser/mod.rs`, unchanged. Add `pub mod python;`.
- Create: `src/cli/commands/index/parser/python/mod.rs`, `.../python/tests.rs`

**Interfaces:**
- Consumes: Task 1 types, `module_name`, `node_id`.
- Produces: `pub fn parse_python(project: &str, relative_path: &str, source: &str) -> PythonParse`, where
  `pub struct PythonParse { pub graph: FileGraph, pub error_lines: Vec<usize> }`.
  `graph.hash` is `0`. `error_lines` is filled in Task 4; this task leaves it empty.

**Rules to implement** (from the spec):
- **Language setup:** `parser.set_language(&tree_sitter_python::LANGUAGE.into())`, then `parser.parse(source, None)`. A `None` result is an empty module-only graph here. Task 4 turns it into a failure.
- **Definitions:**
  - `class_definition` → `Class`.
  - `function_definition` directly in a class body → `Method`; any other → `Function`. `async def` is also a `function_definition`.
  - A `decorated_definition` takes its kind from its `definition` field, and its span from the decorated node, so the span starts at the first decorator.
- **Blocks:** definitions inside `if`, `try`, `with`, `for`, `while` and `match` blocks belong to the enclosing scope. Walk named children recursively, but only open a new scope at a class or function.
- **Edges:** a `Contains` edge from each scope node to each definition directly inside it.
- **Duplicate names** in one scope: the second gets `#2`, the third `#3`, and so on. Applied to the qualified name, so children are `mod.f#2.inner`.
- **Stubs:** for a path ending in `.pyi`, every node id ends with `@pyi`, after any `#n`. Example: `api:pkg.x.Foo@pyi`, and the module is `api:pkg.x@pyi`.
- **Spans:** `start_position().row + 1` through `end_position().row + 1`. The module span is `1..=max(line count, 1)`.
- **Names:** from the `name` field via `utf8_text`.
- **Node order** in `graph.nodes`: the module first, then source order.

- [ ] **Step 1: Write the failing tests** in `python/tests.rs`. Use a helper `ids(&FileGraph) -> Vec<(&str, NodeKind)>` and `contains(&FileGraph) -> Vec<(&str, &str)>` (from/to ids).

```rust
#[test]
fn classes_methods_and_nested_functions() {
    let source = "class Invoice:\n    def total(self):\n        def round_it(x):\n            return x\n        return 1\n\nasync def fetch():\n    pass\n";
    let graph = parse_python("api", "billing/invoices.py", source).graph;
    assert_eq!(ids(&graph), [
        ("api:billing.invoices", NodeKind::Module),
        ("api:billing.invoices.Invoice", NodeKind::Class),
        ("api:billing.invoices.Invoice.total", NodeKind::Method),
        ("api:billing.invoices.Invoice.total.round_it", NodeKind::Function),
        ("api:billing.invoices.fetch", NodeKind::Function),
    ]);
    assert_eq!(contains(&graph), [
        ("api:billing.invoices", "api:billing.invoices.Invoice"),
        ("api:billing.invoices.Invoice", "api:billing.invoices.Invoice.total"),
        ("api:billing.invoices.Invoice.total", "api:billing.invoices.Invoice.total.round_it"),
        ("api:billing.invoices", "api:billing.invoices.fetch"),
    ]);
}

#[test]
fn decorated_definitions_span_their_decorators() {
    let graph = parse_python("api", "app.py", "@cache\n@log\ndef run():\n    pass\n").graph;
    assert_eq!(graph.nodes[1].id, "api:app.run");
    assert_eq!(graph.nodes[1].span, Span { start_line: 1, end_line: 4 });
}

#[test]
fn definitions_in_blocks_belong_to_the_enclosing_scope() {
    let source = "if TYPE_CHECKING:\n    def f(): ...\nelse:\n    def f(): ...\ntry:\n    class C: ...\nexcept ImportError:\n    pass\n";
    let graph = parse_python("api", "m.py", source).graph;
    assert_eq!(ids(&graph).iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["api:m", "api:m.f", "api:m.f#2", "api:m.C"]);
}

#[test]
fn stub_ids_do_not_collide_with_source_ids() {
    let graph = parse_python("api", "pkg/x.pyi", "class Foo: ...\n").graph;
    assert_eq!(graph.module, "api:pkg.x@pyi");
    assert_eq!(graph.nodes[1].id, "api:pkg.x.Foo@pyi");
}

#[test]
fn empty_and_comment_only_files_have_just_the_module() {
    for source in ["", "# only a comment\n"] {
        let graph = parse_python("api", "empty.py", source).graph;
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].span.start_line, 1);
    }
}

#[test]
fn non_ascii_names_and_crlf_lines() {
    let graph = parse_python("api", "m.py", "x = 1\r\n\r\ndef café():\r\n    pass\r\n").graph;
    assert_eq!(graph.nodes[1].name, "café");
    assert_eq!(graph.nodes[1].span, Span { start_line: 3, end_line: 4 });
}
```

- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test parser::python`. Expected: a compile error, because `parse_python` is not defined.
- [ ] **Step 3: Implement `parse_python`** in `python/mod.rs`. Use a scope stack of `(qualified_name, node_id, is_class)` and a per-scope `HashMap<String, usize>` of name counts for the `#n` suffixes.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test parser::python`. Expected: 6 passed.
- [ ] **Step 5: Commit.** `git add Cargo.toml Cargo.lock src/cli/commands/index/parser && git commit -m "Parse Python definitions into graph nodes"`

---

### Task 3: Imports and base classes → references

**Files:**
- Modify: `src/cli/commands/index/parser/python/mod.rs`, `.../python/tests.rs`

**Interfaces:**
- Consumes: Task 2's `parse_python`.
- Produces: `graph.references`, in source order. `from` is the id of the scope node the statement appears in. `line` is the statement's start line.

**Rules** (from the spec):
- **Import targets** (`Imports`):
  - `import a.b` and `import a.b as c` → `a.b`.
  - `from a.b import c, d as e` → `a.b.c` and `a.b.d`, one reference per name.
  - `from a.b import *` (a `wildcard_import` child) → `a.b`.
- **Relative imports** (`relative_import`, an `import_prefix` of n dots plus an optional `dotted_name`) resolve against the file's package:
  - The package is the module name without its last part. For an `__init__.py`, it is the module itself.
  - Each dot after the first goes up one level.
  - If that goes above the top level, keep the target exactly as written, e.g. `...x`.
- **Name map**, built in a first pass over the whole file before resolving bases. Nested imports inside functions are not included.
  - `import a.b` binds `a` → `a`.
  - `import a.b as c` binds `c` → `a.b`.
  - `from m import n as k` binds `k` (or `n`) → the resolved `m.n`.
  - Each module-level class binds its name → its qualified name, without the `@pyi` suffix.
- **Base classes** (`Inherits`, from the class node). For each entry in `superclasses`:
  - Skip `keyword_argument` entries such as `metaclass=M`.
  - For `subscript` entries like `Generic[T]`, use the value before `[`.
  - Take the dotted text, map its first segment through the name map if bound, and keep the rest. For example, `abc.ABC` with `import abc` gives `abc.ABC`.
  - Unbound names stay as written: `Exception`.

- [ ] **Step 1: Write the failing tests.** Use a helper `refs(&FileGraph) -> Vec<(&str, &str, ReferenceKind)>` (from, target, kind).

```rust
#[test]
fn every_import_form() {
    let source = "import os.path\nimport numpy as np\nfrom a.b import c, d as e\nfrom m import *\n\ndef f():\n    import json\n";
    let graph = parse_python("api", "pkg/mod.py", source).graph;
    assert_eq!(refs(&graph), [
        ("api:pkg.mod", "os.path", ReferenceKind::Imports),
        ("api:pkg.mod", "numpy", ReferenceKind::Imports),
        ("api:pkg.mod", "a.b.c", ReferenceKind::Imports),
        ("api:pkg.mod", "a.b.d", ReferenceKind::Imports),
        ("api:pkg.mod", "m", ReferenceKind::Imports),
        ("api:pkg.mod.f", "json", ReferenceKind::Imports),
    ]);
    assert_eq!(graph.references[5].line, 7);
}

#[test]
fn relative_imports_resolve_against_the_package() {
    let module = parse_python("api", "pkg/sub/mod.py", "from .x import y\nfrom .. import z\nfrom . import w\n").graph;
    assert_eq!(targets(&module), ["pkg.sub.x.y", "pkg.z", "pkg.sub.w"]);
    let init = parse_python("api", "pkg/sub/__init__.py", "from .x import y\n").graph;
    assert_eq!(targets(&init), ["pkg.sub.x.y"]);
}

#[test]
fn relative_imports_above_the_top_package_keep_their_dots() {
    let graph = parse_python("api", "a/b.py", "from ... import x\nfrom ...c import d\n").graph;
    assert_eq!(targets(&graph), ["...x", "...c.d"]);
}

#[test]
fn base_classes_resolve_through_imports_and_local_classes() {
    let source = "import abc\nfrom models import Base as B\nfrom typing import Generic\n\nclass Local: ...\nclass C(B, Local, abc.ABC, Exception, Generic[T], metaclass=Meta): ...\n";
    let graph = parse_python("api", "m.py", source).graph;
    let inherits: Vec<_> = refs(&graph).into_iter().filter(|(_, _, kind)| *kind == ReferenceKind::Inherits).collect();
    assert_eq!(inherits, [
        ("api:m.C", "models.Base", ReferenceKind::Inherits),
        ("api:m.C", "m.Local", ReferenceKind::Inherits),
        ("api:m.C", "abc.ABC", ReferenceKind::Inherits),
        ("api:m.C", "Exception", ReferenceKind::Inherits),
        ("api:m.C", "typing.Generic", ReferenceKind::Inherits),
    ]);
}
```

`targets(&FileGraph) -> Vec<&str>` lists the targets of every reference.

- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test parser::python`. Expected: the 4 new tests fail on empty `references`.
- [ ] **Step 3: Implement the first pass (name map) and reference collection** in `python/mod.rs`. Keep import handling in a helper `fn import_targets(node, module: &str, is_package: bool, source: &[u8]) -> Vec<(String, Option<String>)>`, which returns the target and the local binding.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test parser::python`. Expected: 10 passed.
- [ ] **Step 5: Commit.** `git commit -am "Record Python imports and base classes as references"`

---

### Task 4: Partial graphs, the new `Parser` trait, and graphs through `workbench index`

Changing the trait breaks the index module until it is updated, so both changes are one task and one commit.

**Files:**
- Modify: `src/cli/commands/index/parser/mod.rs`, `.../python/mod.rs`, `.../python/tests.rs`
- Modify: `src/cli/commands/index/storage.rs`, `src/cli/commands/index/mod.rs`, `src/cli/commands/index/tests.rs`, `README.md`

**Interfaces:**
- Consumes: `parse_python`, `FileGraph::module_only`.
- Produces:
  - `pub trait Parser { fn parse(&self, project: &str, relative_path: &str, contents: &[u8]) -> ParseOutcome; }`
  - `pub enum ParseOutcome { Parsed(FileGraph), Partial(FileGraph, String), Failed(String) }`
  - `pub struct PythonParser;` implements `Parser`.
  - `TextParser` implements `Parser`: valid UTF-8 → `Parsed(FileGraph::module_only(project, relative_path, line count))`; invalid → `Failed("not valid UTF-8 text: {error}")`.
- Remove the `#[cfg_attr(not(test), expect(dead_code, ...))]` on `Partial`. It is constructed now, and an unfulfilled `expect` fails clippy.

**Rules:**
- **`error_lines`:** the sorted, de-duplicated 1-based start lines of every node where `is_error()` or `is_missing()`.
- **`PythonParser::parse`:**
  - Not UTF-8 → `Failed("not valid UTF-8 text: {error}")`.
  - tree-sitter returns `None` → `Failed("the parser gave up")`.
  - Non-empty `error_lines` → `Partial(graph, "syntax errors at lines 3, 7")`.
  - Otherwise → `Parsed(graph)`.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn syntax_errors_give_a_partial_graph_of_the_rest() {
    let source = "def ok():\n    pass\n\ndef broken(:\n    pass\n\nclass Fine: ...\n";
    let ParseOutcome::Partial(graph, details) = PythonParser.parse("api", "m.py", source.as_bytes()) else {
        return Err("expected a partial parse".into());
    };
    assert!(graph.nodes.iter().any(|node| node.id == "api:m.ok"));
    assert!(graph.nodes.iter().any(|node| node.id == "api:m.Fine"));
    assert!(details.starts_with("syntax errors at lines 4"));
    Ok(())
}

#[test]
fn invalid_utf8_fails() {
    assert!(matches!(PythonParser.parse("api", "m.py", &[0xff, 0xfe]), ParseOutcome::Failed(details) if details.starts_with("not valid UTF-8")));
}

#[test]
fn clean_files_parse_fully() {
    assert!(matches!(PythonParser.parse("api", "m.py", b"x = 1\n"), ParseOutcome::Parsed(_)));
}
```

The first test returns `Result<(), Box<dyn std::error::Error>>`.

**Index integration (same task)**

**Interfaces:**
- Consumes: Task 4's `Parser`, `ParseOutcome`, `PythonParser`, `TextParser`, and Task 1's `FileGraph`.
- Produces:
  - `ProjectChanges { pub upserted: Vec<FileGraph>, pub removed: Vec<String> }`. `IndexedFile` is deleted.
  - In `Context`, the field `parser` is replaced by `pub python_parser: &'a dyn Parser` and `pub typescript_parser: &'a dyn Parser`.
  - `run` passes `&PythonParser` and `&TextParser`.

**Rules:**
- **Parser choice:** `index_project` picks the parser from `project.language`: `"python"` → `python_parser`, `"typescript"` → `typescript_parser`. An unknown language already fails earlier, unchanged.
- **Per file:**
  - `index_file` calls `parser.parse(&project.id, &index_path(relative), &contents)`.
  - On `Parsed` or `Partial` it sets `graph.hash = hash` and pushes the graph.
  - The log line for a partial file keeps its current wording, with the details from the parser.
- **Unchanged-file hash comparison:** stays as it is.

- [ ] **Step 2: Update the index fakes and add the failing integration test** in `index/tests.rs`.
  - `FakeStorage::write_project` clones `changes.upserted` (now `Vec<FileGraph>`).
  - Calls to `index_with` take one parser and pass it as both `python_parser` and `typescript_parser`.
  - `PartialParser::parse` gets the new signature and returns `Partial(FileGraph::module_only(..), ..)`.
  - Add:

```rust
#[test]
fn python_projects_are_parsed_into_graphs() -> CliResult<()> {
    let scratch = Scratch::new("graph", TWO_PROJECTS)?;
    scratch.write("api/billing/models.py", "from .base import Model\n\nclass Invoice(Model):\n    def total(self): ...\n")?;
    let storage = FakeStorage::default();
    let run = scratch.index_with_parsers(&only("api"), &storage, &PythonParser, &TextParser)?;
    assert_eq!(report(&run, "api").map(|r| r.counts.parsed), Some(1));
    let written = storage.written.borrow();
    let graph = &written[0].1.upserted[0];
    assert_eq!(graph.path, "billing/models.py");
    assert_ne!(graph.hash, 0);
    assert!(graph.nodes.iter().any(|node| node.id == "api:billing.models.Invoice.total"));
    assert!(graph.references.iter().any(|r| r.target == "billing.base.Model" && r.kind == ReferenceKind::Inherits));
    Ok(())
}
```

  The test needs `use crate::{graph::ReferenceKind, ...parser::python::PythonParser}`. Add a helper `index_with_parsers(args, storage, python, typescript, interrupted = false)` to `Scratch`, and make `index_with` call it with the same parser twice.
- [ ] **Step 3: Run the tests and see them fail.** Run `cargo test`. Expected: compile errors, because `PythonParser` is not defined, and the `ParseOutcome`, `ProjectChanges` and `Context` shapes are new.
- [ ] **Step 4: Implement** `error_lines`, the new trait shape, `PythonParser` and the updated `TextParser`. Then update `storage.rs` and `index/mod.rs`.
- [ ] **Step 5: Run everything.** Run `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo test --workspace --all-features --locked`. Expected: no warnings, and all tests pass: the 63 existing ones plus 18 new.
- [ ] **Step 6: Update `README.md`.** In the `workbench index` placeholder note, replace "Parsing currently only checks that a file is valid UTF-8 text" with "Python files are parsed into a code graph of modules, classes, functions and methods, with their imports and base classes; TypeScript files are only checked to be valid UTF-8 text. Nothing is stored yet." Add `src/graph/` to the source tree: `graph/  # language-neutral code graph`.
- [ ] **Step 7: Run on a real project.** Index a scratch workspace whose Python project has a class, a relative import and a file with a syntax error. Expected: the summary shows `partial` = 1, the log names the error line, and the exit code is 0.
- [ ] **Step 8: Commit.** `git add -A && git commit -m "Parse Python projects into code graphs during index"`. No attribution lines.

---

## After the plan

Built after the four tasks, once the parser ran on a real project (`test-project`, project `demo`, 36 files):

- **`workbench index --log-graph`** writes every parsed file's nodes, edges and references to `.workbench/logs/workbench-YYYY-MM-DD.log`, so a real run can be checked by reading the graph.
- **`src` stays in module names when `src/` is a package.** The project imports its own code as `src.config`, `src.models…` because `src/` has an `__init__.py`. Dropping `src/` gave ids that no import could match. Now `index` checks for `src/__init__.py` (or `.pyi`) once per project, and parsers receive a `SourceFile { project, path, src_is_package }`. The spec records the changed rule.

Result on the real run:
- every `src.*` import matches a module id or a member of one;
- 27 of 31 project-internal targets match a node id exactly.

## Next steps

> **Done:** items 1–6 were fixed by `docs/superpowers/plans/2026-11-14-parser-fixes.md` (spec `docs/superpowers/specs/2026-11-14-parser-fixes-design.md`).

Settle these before or alongside the link step and the Memgraph writer. They are ordered by how much they affect resolving references.

1. **Module ids can collide within a project.** For example, `pkg/x.py` and `pkg/x/__init__.py` both become `pkg.x`; `x.py` and `src/x.py` (in a plain src layout) both become `x`. In TypeScript, `a.ts` and `a.js`, or `index.ts` and `index.tsx`, collide too. This is harmless while storage is a placeholder, but it must be settled before the Memgraph writer, since ids are the nodes' keys.
2. **Module-level variables have no nodes.** On the real run, 4 of 31 project-internal targets are imports of variables, so there is nothing to link them to:
   - `src.config.settings`
   - `src.database.database.METADATA`
   - `src.database.tables.item_tables.item_table`
   - `src.database.tables.user_tables.user_table`

   Decide whether variables (at least module-level assignments) become nodes, or whether such references resolve to the module.
3. **Duplicate imports resolve to the wrong binding.** If a name is imported twice at module level, as in an `if` block and again later, a base class may resolve to the earlier import, not the last one in source order. The name map walks depth-first; it should follow source order.
4. **Base classes defined inside functions aren't resolved.** The plan only covered module-level classes. The spec says a base defined in the same file resolves to that node's qualified name, e.g. `jobs.run.Inner`.
5. **A base that names a class defined twice ignores its suffix.** For example, `class B: ...` twice, then `class C(B)`, gives `m.B` rather than `m.B#2`, the definition that is in effect.
6. **Spaces inside a dotted base stay in the target.** `class C(a . b . X)` gives `a . b . X`. Build the dotted name from the identifiers instead of the raw text.

Smaller cases the review set aside, kept as they are for now:
- `from __future__ import` records no reference;
- non-UTF-8 files that declare an encoding (`# coding: latin-1`) count as failed;
- files with bare `\r` line endings get a one-line module span.
