# Code Graph Parser Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make module ids unique, add module-level `Variable` nodes, and resolve base classes through the bindings in effect where each class is defined.

**Architecture:**
- **Dotted bases:** built from identifier nodes.
- **Base resolution:** the walk in `parser/python/mod.rs` keeps a stack of binding frames, filled in source order. This replaces the whole-file name map.
- **Variables:** come from module-scope assignments.
- **Collisions:** `index` computes module-id collisions across all of a project's files. It passes each losing file's path to the parser through `SourceFile.disambiguator`, and the parser appends `@<path>` to every id in that file.

**Tech Stack:** Rust 2024, tree-sitter 0.27.2, tree-sitter-python 0.25.0.

**Spec:** `docs/superpowers/specs/2026-11-14-parser-fixes-design.md`

## Global Constraints

- **Node ids:** `<project-id>:<qualified name>`. Suffix order within an id: the qualified name, then `#n`, then `@pyi` or `@<path>`.
- **Lines:** 1-based and inclusive.
- **Clippy** with `-D warnings` must pass; `unwrap_used` and `panic` are denied, also in tests (return `Result` instead). Verify without `--all-features`.
- **Tests** live in a separate `tests.rs` beside each module's `mod.rs`.
- **The walk** never descends into expressions. `STATEMENT_CONTAINERS` stays the only way down, apart from class and function bodies.
- **Commit messages** carry no Claude attribution or any other trace of Claude.

## Review Focus

1. **A class defined inside a function using a sibling class as its base, after a module class of the same name** (`class A` at module level, then `def f(): class A; class B(A)`): `B` inherits the function's `A`, `m.f.A`. Tested in Task 2.
2. **Variable assigned in both branches of an `if`** (`if X: v = 1` / `else: v = 2`): exactly one `Variable` node, at the first assignment's line. Tested in Task 3.
3. **A module-level name both assigned and defined** (`def f(): ...` then `f = wrap(f)`): only the `Function` node, with no duplicate `f` id. Tested in Task 3.
4. **A collision between a `.pyi` stub and a stub package** (`pkg/x.pyi` and `pkg/x/__init__.pyi`): the package stub keeps `@pyi`, and the loser's ids end in `@pkg/x.pyi` with no `@pyi`. Tested in Task 4.
5. **`--changed` re-parses only the loser of a collision:** the loser still gets its `@<path>` suffix, since collisions are computed from all files. Tested in Task 4.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/cli/commands/index/parser/python/mod.rs` (modify) | Dotted names, binding frames, variables, id suffix |
| `src/cli/commands/index/parser/python/tests.rs` (modify) | Parser tests for Tasks 1–4 |
| `src/graph/mod.rs`, `src/graph/tests.rs` (modify) | `NodeKind::Variable`, `SourceFile.disambiguator`, suffix in `module_only` |
| `src/cli/commands/index/collisions/mod.rs` (create) | Which files lose a module-id collision |
| `src/cli/commands/index/collisions/tests.rs` (create) | Precedence-rule tests |
| `src/cli/commands/index/mod.rs`, `tests.rs` (modify) | Compute collisions per project, pass disambiguators, log them |
| `README.md`, the first plan's "Next steps" (modify) | Document the new rules, and mark the steps done |

---

### Task 1: Dotted bases from identifiers

**Files:** `parser/python/mod.rs`, `parser/python/tests.rs`

**Interfaces:**
- Produces: `fn dotted_name(node: Syntax<'_>, module: &ModuleContext<'_>) -> Option<String>`.
  - `identifier` → its text.
  - `attribute` → the `dotted_name` of its `object` field, `.`, then its `attribute` field's text.
  - Anything else → `None`.
- `base_classes` uses `dotted_name` instead of `module.text(base)`. For a `subscript`, it applies to the `value` field, as today.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn spaces_inside_dotted_bases_are_ignored() {
    let graph = parse_python("api", "m.py", "import a.b\nclass C(a . b . X, (lambda: Base)()): ...\n").graph;
    let inherits: Vec<_> = refs(&graph).into_iter().filter(|(_, _, kind)| *kind == ReferenceKind::Inherits).collect();
    assert_eq!(inherits, [("api:m.C", "a.b.X", ReferenceKind::Inherits)]);
}
```

- [ ] **Step 2: Run the test and see it fail.** Run `cargo test parser::python::tests::spaces`. Expected: FAIL; the target is `a . b . X`.
- [ ] **Step 3: Implement `dotted_name`,** and use it in `base_classes`.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test parser::python`. Expected: all pass.
- [ ] **Step 5: Commit.** `git commit -am "Build dotted base names from identifiers"`

---

### Task 2: Resolve bases through scope bindings, in source order

**Files:** `parser/python/mod.rs`, `parser/python/tests.rs`

**Interfaces:**
- Consumes: Task 1's `dotted_name`.
- Produces, in `parser/python/mod.rs`:
  - `struct Frame { names: HashMap<String, String>, is_class: bool }`.
  - `Walk` gains `frames: Vec<Frame>`. `frames[0]` is the module and is pushed before the walk starts.
  - `fn bind(&mut self, local: String, dotted: String)` inserts into the last frame; a later bind replaces an earlier one.
  - `fn resolve(&self, first: &str) -> Option<&str>` looks up:
    1. the last frame;
    2. the frames below it down to index 1, skipping those with `is_class`;
    3. `frames[0]`.
  - `Walk.names` is removed, and so are `name_map`, `module_level_statements`, `opens_scope` and `bindings`.
  - `base_classes` becomes a `Walk` method, `fn base_classes(&self, class: Syntax<'_>) -> Vec<String>`. It resolves each base's first segment with `self.resolve`, and keeps the rest unchanged.

**Rules:**
- **Imports:** when the walk visits an import, it binds each `Import.binding`, in addition to recording the reference.
- **Definitions:** in `define`, the class's bases are resolved first, then the name is bound to its qualified name, `#n` included, in the current (enclosing) frame. Functions bind their name the same way.
- **Scopes:** then a frame `{ is_class: kind == Class }` is pushed for the body, and popped after it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn bases_use_the_last_binding_in_source_order() {
    let source = "if A:\n    from a import Base\nfrom b import Base\nclass C(Base): ...\nfrom c import Base\n";
    assert_eq!(inherits_targets(&parse_python("api", "m.py", source).graph), ["b.Base"]);
}

#[test]
fn bases_defined_inside_functions_resolve_to_their_scope() {
    let source = "class A: ...\ndef f():\n    class A: ...\n    class B(A): ...\n";
    assert_eq!(inherits_targets(&parse_python("api", "m.py", source).graph), ["m.f.A"]);
}

#[test]
fn bases_name_the_definition_in_effect() {
    let source = "class B: ...\nclass B: ...\nclass C(B): ...\n";
    assert_eq!(inherits_targets(&parse_python("api", "m.py", source).graph), ["m.B#2"]);
}

#[test]
fn class_bodies_do_not_enclose_nested_functions() {
    let source = "class Outer:\n    class Inner: ...\n    def method(self):\n        class Local(Inner): ...\n";
    assert_eq!(inherits_targets(&parse_python("api", "m.py", source).graph), ["Inner"]);
}

#[test]
fn local_imports_bind_inside_their_function() {
    let source = "def f():\n    from models import Base\n    class C(Base): ...\nclass D(Base): ...\n";
    assert_eq!(inherits_targets(&parse_python("api", "m.py", source).graph), ["models.Base", "Base"]);
}
```

  Add a helper `inherits_targets(&FileGraph) -> Vec<&str>`, which lists the targets of the `Inherits` references in order.

- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test parser::python`. Expected: the first four fail (the targets are `a.Base`, `m.A`, `m.B` and `Inner`, or wrong in other ways); the fifth may already pass.
- [ ] **Step 3: Implement the frames,** and remove the whole-file name map.
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test parser::python`. Expected: all pass, including the earlier base-class tests (`base_classes_resolve_through_imports_and_local_classes` and `decorated_local_classes_resolve_as_bases`).
- [ ] **Step 5: Commit.** `git commit -am "Resolve base classes through scope bindings in source order"`

---

### Task 3: Module-level Variable nodes

**Files:** `src/graph/mod.rs`, `src/graph/tests.rs`, `parser/python/mod.rs`, `parser/python/tests.rs`

**Interfaces:**
- Consumes: Task 2's `bind`.
- Produces:
  - `NodeKind::Variable`, which `FileGraph::describe` prints as `Variable`.
  - In the parser: `fn assigned_names(assignment: Syntax<'_>, module: &ModuleContext<'_>) -> Vec<String>`.
  - `Walk` gains `variables: Vec<(String, Span)>`, recording each name's first assignment in source order.

**Rules:**
- **Where:** in `visit_child`, an `expression_statement` is handled only while the walk is in module scope (`frames.len() == 1`). Each `assignment` child is handled.
- **Which names (`assigned_names`):**
  - the `left` field: an `identifier` gives its name;
  - `pattern_list`, `tuple_pattern`, `list_pattern` and `list_splat_pattern` give their identifiers, recursively;
  - `attribute` and `subscript` give nothing;
  - if `right` is itself an `assignment`, its names are added too, so `a = b = 1` gives both.
- **Binding and recording:** each name is bound to `join(module, name)` right away, in source order, so `Base = declarative_base()` resolves later bases. It is recorded in `variables` only if not already there, so its span is the first assignment's statement.
- **Nodes:** after the walk, each recorded variable whose name has no `Class` or `Function` node directly in the module becomes a `Variable` node. The check uses the `name` field of nodes the module `Contains`. The node's id is `node_id(project, join(module, name))` plus the file's suffix, and it gets a `Contains` edge from the module. Variable nodes are appended after the definitions, in source order.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn module_level_assignments_become_variables() {
    let source = "settings = get_settings()\nMETADATA: MetaData = MetaData()\nT: TypeVar\na, (b, *rest) = 1, (2, 3)\nx = y = 0\nobj.attr = 1\nd[k] = 2\ncount += 1\ndef f():\n    local = 1\nclass C:\n    attr = 1\n";
    let graph = parse_python("api", "config.py", source).graph;
    let variables: Vec<(&str, usize)> = graph.nodes.iter()
        .filter(|node| node.kind == NodeKind::Variable)
        .map(|node| (node.id.as_str(), node.span.start_line))
        .collect();
    assert_eq!(variables, [
        ("api:config.settings", 1), ("api:config.METADATA", 2), ("api:config.T", 3),
        ("api:config.a", 4), ("api:config.b", 4), ("api:config.rest", 4),
        ("api:config.x", 5), ("api:config.y", 5),
    ]);
    assert!(contains(&graph).contains(&("api:config", "api:config.settings")));
}

#[test]
fn a_variable_assigned_in_both_branches_is_one_node() {
    let graph = parse_python("api", "m.py", "if X:\n    v = 1\nelse:\n    v = 2\n").graph;
    let variables: Vec<_> = graph.nodes.iter().filter(|node| node.kind == NodeKind::Variable).collect();
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].span.start_line, 2);
}

#[test]
fn definitions_win_over_variables_of_the_same_name() {
    let graph = parse_python("api", "m.py", "def f(): ...\nf = wrap(f)\n").graph;
    assert_eq!(ids(&graph), [("api:m", NodeKind::Module), ("api:m.f", NodeKind::Function)]);
}

#[test]
fn variables_bind_names_for_bases() {
    let graph = parse_python("api", "db.py", "Base = declarative_base()\nclass User(Base): ...\n").graph;
    assert_eq!(inherits_targets(&graph), ["db.Base"]);
}
```

  In `graph/tests.rs`, extend `describe_lists_nodes_edges_and_references`, or add a test, asserting that a `Variable` node prints as `  Variable api:m.x (lines 1-1)`.
- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test parser::python graph::`. Expected: a compile error, because `NodeKind::Variable` does not exist.
- [ ] **Step 3: Implement `NodeKind::Variable`, `assigned_names`, and the variable collection and node emission.**
- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test`. Expected: all pass.
- [ ] **Step 5: Commit.** `git commit -am "Add module-level variables as graph nodes"`

---

### Task 4: Unique module ids across a project

**Files:**
- Create: `src/cli/commands/index/collisions/mod.rs`, `src/cli/commands/index/collisions/tests.rs`
- Modify: `src/graph/mod.rs`, `src/graph/tests.rs`, `parser/python/mod.rs`, `parser/python/tests.rs`, `src/cli/commands/index/mod.rs`, `src/cli/commands/index/tests.rs`, `README.md`, `docs/superpowers/plans/2026-11-14-python-ast-graph.md`

**Interfaces:**
- **In `graph`:**
  - `SourceFile` gains `pub disambiguator: Option<&'a str>`: the file's own path when it lost a collision, otherwise `None`.
  - `FileGraph::module_only` appends `@<disambiguator>` to the module id when it is set.
- **In the parser:** `Walk.stub: bool` becomes `suffix: String`:
  - `@<path>` when there is a disambiguator;
  - otherwise `@pyi` for `.pyi` files;
  - otherwise empty.

  Every node id, the module id included, gets `suffix`.
- **In `collisions/mod.rs`:**
  - `pub struct ModuleFile { pub path: String, pub module: String, pub stub: bool }`.
  - `pub fn losers(files: &[ModuleFile], extensions: &[&str], src_is_package: bool) -> Vec<Collision>`.
  - `pub struct Collision { pub module: String, pub winner: String, pub losers: Vec<String> }`. Paths are relative to the project, with `/` separators.
- **In `index/mod.rs`:**
  - `ProjectFiles` gains `disambiguated: &'a HashSet<String>`, the losers' paths.
  - `index_file` sets `SourceFile.disambiguator = Some(&key)` when the file's path is in the set.

**Precedence:** `losers` groups files by `(module, stub)`. Within a group of two or more, files are sorted by these keys, in order, and the first wins:
1. a package file first: the file name's stem is `__init__`;
2. when `!src_is_package`, a path under `src/` first;
3. the position of the file's extension in `extensions`;
4. the path, alphabetically.

**`index` flow:**
- **Before parsing a project,** list all its source files with `discover::source_files`, even with `--changed`. Build a `ModuleFile` for each one, from `SourceFile { .. }.module_name()` and whether it ends in `.pyi`, then call `losers`.
- **Logging:** for each `Collision`, log one `INFO` line:
  `module `<module>` is defined by <winner> and <loser1>, <loser2>; ids from the others get the suffix @<path>`.

- [ ] **Step 1: Write the failing tests**

  In `collisions/tests.rs`:

```rust
fn file(path: &str, module: &str) -> ModuleFile {
    ModuleFile { path: path.to_owned(), module: module.to_owned(), stub: path.ends_with(".pyi") }
}

#[test]
fn a_package_wins_over_a_module() {
    let found = losers(&[file("pkg/x.py", "pkg.x"), file("pkg/x/__init__.py", "pkg.x")], &["py", "pyi"], false);
    assert_eq!(found, [Collision { module: "pkg.x".to_owned(), winner: "pkg/x/__init__.py".to_owned(), losers: vec!["pkg/x.py".to_owned()] }]);
}

#[test]
fn a_plain_src_file_wins_over_a_root_script() {
    let found = losers(&[file("x.py", "x"), file("src/x.py", "x")], &["py", "pyi"], false);
    assert_eq!(found[0].winner, "src/x.py");
}

#[test]
fn earlier_extensions_win_then_paths() {
    let found = losers(&[file("a.js", "a"), file("a.tsx", "a"), file("a.ts", "a")], &["ts", "tsx", "js"], false);
    assert_eq!((found[0].winner.as_str(), found[0].losers.clone()), ("a.ts", vec!["a.tsx".to_owned(), "a.js".to_owned()]));
}

#[test]
fn a_stub_package_wins_over_a_stub_module() {
    let found = losers(&[file("pkg/x.pyi", "pkg.x"), file("pkg/x/__init__.pyi", "pkg.x")], &["py", "pyi"], false);
    assert_eq!((found[0].winner.as_str(), found[0].losers.clone()), ("pkg/x/__init__.pyi", vec!["pkg/x.pyi".to_owned()]));
}

#[test]
fn stubs_only_collide_with_stubs() {
    let found = losers(&[file("pkg/x.py", "pkg.x"), file("pkg/x.pyi", "pkg.x")], &["py", "pyi"], false);
    assert!(found.is_empty());
}
```

  In `graph/tests.rs`, so that TypeScript placeholder graphs get the suffix too:

```rust
#[test]
fn module_only_graphs_carry_the_disambiguator() {
    let file = SourceFile { project: "web", path: "a.js", src_is_package: false, disambiguator: Some("a.js") };
    assert_eq!(FileGraph::module_only(&file, 1).module, "web:a@a.js");
}
```

  In `python/tests.rs`:

```rust
#[test]
fn a_disambiguated_file_suffixes_every_id_with_its_path() {
    let file = SourceFile { project: "api", path: "pkg/x.pyi", src_is_package: false, disambiguator: Some("pkg/x.pyi") };
    let graph = parse_file(&file, "class Foo: ...\nv = 1\n").graph;
    assert_eq!(graph.module, "api:pkg.x@pkg/x.pyi");
    assert_eq!(graph.nodes[1].id, "api:pkg.x.Foo@pkg/x.pyi");
    assert_eq!(graph.nodes[2].id, "api:pkg.x.v@pkg/x.pyi");
}
```

  In `index/tests.rs`:

```rust
#[test]
fn collisions_are_computed_from_every_file_even_with_changed() -> CliResult<()> {
    let scratch = Scratch::new("collisions", TWO_PROJECTS)?;
    let api = scratch.0.join("api");
    scratch.write("api/pkg/x/__init__.py", "")?;
    scratch.write("api/pkg/x.py", "class Old: ...\n")?;
    git(&api, &["init", "-q"])?;
    git(&api, &["add", "."])?;
    git(&api, &["commit", "-q", "-m", "initial"])?;
    scratch.write("api/pkg/x.py", "class New: ...\n")?;
    let storage = FakeStorage::default();
    scratch.run(&IndexArgs { changed: true, ..only("api") }, &Run { storage: &storage, python: &PythonParser, typescript: &TextParser, interrupted: false })?;
    let written = storage.written.borrow();
    let graphs = &written[0].1.upserted;
    assert_eq!(graphs.len(), 1);
    assert_eq!(graphs[0].module, "api:pkg.x@pkg/x.py");
    assert!(scratch.log().contains("module `pkg.x` is defined by pkg/x/__init__.py and pkg/x.py"));
    Ok(())
}
```

  Update every existing `SourceFile { .. }` literal with `disambiguator: None`.
- [ ] **Step 2: Run the tests and see them fail.** Run `cargo test`. Expected: compile errors, because `collisions`, `ModuleFile` and `disambiguator` don't exist.
- [ ] **Step 3: Implement the `collisions` module, the `SourceFile.disambiguator` field and its suffix, and the `index` flow and logging.**
- [ ] **Step 4: Run everything.** Run `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --locked`. Expected: no warnings, and all tests pass.
- [ ] **Step 5: Update the docs.**
  - In `README.md`, under **Python parsing**, add:
    - module-level variables are `Variable` nodes;
    - when two files of a project give the same module name, the file Python would load keeps the id and the others get `@<path>`, with a log line;
    - base classes resolve to the binding in effect where the class is defined.
  - In `docs/superpowers/plans/2026-11-14-python-ast-graph.md` "Next steps", mark items 1–6 done, with a pointer to this plan.
- [ ] **Step 6: Re-run on a copy of `test-project`.** Run `index --id demo --log-graph`. Expected: all 31 project-internal `src.*` targets match a node id, including `src.config.settings`, `src.database.database.METADATA`, `src.database.tables.item_tables.item_table` and `src.database.tables.user_tables.user_table`; and the exit code is 0.
- [ ] **Step 7: Commit.** `git add -A src README.md docs && git commit -m "Keep module ids unique across a project"`. No attribution.
