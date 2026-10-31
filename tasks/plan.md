# Implementation Plan: Slice A (workspace model + `init`, `project`, discovery)

Specs: [`SPEC-workspace.md`](../docs/spec/SPEC-workspace.md) and Slice A of [`SPEC-cli.md`](../docs/spec/SPEC-cli.md) (both approved). Task details and acceptance criteria are in [`todo.md`](todo.md). Design: [HLD](../docs/hld/HLD-workbench.md) (system) and [LLD](../docs/lld/LLD-workspace-cli.md) (types, ports, algorithms for this slice).

## Overview

Build the whole `workspace` module (model, validation, YAML store, memory layout, scope, LSP precedence) and, on top of it, the CLI commands `workbench init`, `project add | list | remove`, workspace discovery (`--workspace`, search upward) and the common error behaviour. Nothing here needs Memgraph, `core`, or the `index`/`semantic` modules.

**Out of Slice A** (per the CLI spec): the `.workbench/mcp/` templates written by `init`, `--index` on `project add` (needs `index`), and the `mcp`, `storage status`, `index`, `watch`, `semantic`, `context`, `platform` groups.

## Starting point (checked in the code)

- `lib/engine`: `Platform`, `Error { AlreadyExists, Storage }`, port `ConfigStore::create`, use case `init(store, platforms)`. Depends on `thiserror` only.
- `services/cli`: clap `Init { platform, out }`, `DirFileStore` (creates an *empty* `config.yml`), `main.rs` printing results.
- No `lib/adapters`, no `tasks/`, no integration tests. `Makefile` is empty.
- `serde` and `yaml_serde` are workspace dependencies; `clap` is the only dependency of `services/cli`.

## Architecture decisions

1. **Engine stays free of `serde`.** The YAML adapter in `lib/adapters` owns serde DTOs (`deny_unknown_fields`) and maps them to engine types. Rationale: the workspace spec makes `adapters` the crate that depends on `serde` and `yaml_serde`, and unknown-key rejection is a parsing concern. Cost: one DTO layer.
2. **Whole config model now, not just what Slice A edits.** `project add` must save a config that may hold hand-written `lsp`, `compose` and `semantic.llm` blocks without losing them, and workspace success criterion 2 requires the round trip. So the model carries every field.
3. **`ConfigStore` is replaced by `WorkspaceStore`** (`load`, `save`, `create_empty`) and the old `init(store, platforms)` is rewritten; `--platform` and `PlatformArg` are removed from `init`. `engine::Platform` stays for `platform-setup`.
4. **Randomness without a new dependency:** `IdSource` is implemented in `lib/adapters` from std (`RandomState`); the engine takes the port, tests inject a fixed suffix.
5. **`pyproject.toml` detection without a TOML crate:** detect `[tool.uv]` / `[tool.poetry]` by matching trimmed header lines (`[tool.uv]`, `[tool.uv.…]`), reading files only. A `toml` dependency is ask-first and not approved. Ceiling: a header inside a multi-line string is a false positive; upgrade to a parser if it ever matters. Mark with a `ponytail:` comment.
6. **Relative path storage:** `project add` stores the path relative to the workspace root (using `..`), or absolute when no relative form exists. The relative-path computation is a pure function in the engine over absolute, already-resolved paths; the CLI/adapter does the `canonicalize`.
7. **Integration tests** use `env!("CARGO_BIN_EXE_workbench")` and `std::process::Command`, each in a fresh temp directory built from `std::env::temp_dir()` plus a unique name (no temp-dir crate, which is ask-first). A tiny helper in `tests/cli.rs` creates and removes it.
8. **`project remove --purge-memory`** prints what it will delete and then deletes (no prompt), as the spec says.

## Dependency graph

```
T1 adapters crate + move DirFileStore
 ├─ T2 ProjectId, Language, DependencyManager ── T3 Project, Environment, LSP merge ── T4 WorkspaceConfig + validation ── T5 Scope
 ├─ T6 memory layout + ports                   (needs T2)
 │
 ├─ T7 YAML WorkspaceStore adapter             (needs T4)
 ├─ T8 MemoryStore + ContextSource adapter     (needs T6)
 ├─ T9 GitignoreStore adapter                  (needs T1)
 └─ T10 ProjectInspector adapter               (needs T2)

T11 init use case (engine)                     (needs T4, T6)
T12 IdSource + `workbench init` end to end     (needs T7, T8, T9, T11)
T13 discovery + `project list`                 (needs T7, T12)
T14 add_project use case                       (needs T4, T6, T10)
T15 `project add` end to end                   (needs T13, T14, T8, T10)
T16 `project remove` end to end                (needs T13, T8)
T17 hardening and docs                         (needs all)
```

Parallel where useful: T2–T6 (engine, pure) are independent of T9 and T10 once T2 exists. Everything else is sequential.

## Task index (details in `todo.md`)

### Phase 1: Foundation
- T1 Adapters crate, `DirFileStore` moved
- T2 Project identifiers and enums
- T3 Project, environment and LSP precedence
- T4 Workspace config and validation
- T5 Project scope
- T6 Memory layout and ports

**Checkpoint A: foundation.** Engine tests pass, clippy clean, `workbench init --out ./tmp` still works.

### Phase 2: Adapters
- T7 YAML workspace store
- T8 Memory and context adapters
- T9 Gitignore adapter
- T10 Project inspector adapter

**Checkpoint B: adapters.** Adapter tests pass against fixtures; round trip equals model.

### Phase 3: Commands (vertical slices)
- T11 `init` use case
- T12 `workbench init` end to end
- T13 Workspace discovery and `project list`
- T14 `add_project` use case
- T15 `workbench project add` end to end
- T16 `workbench project remove` end to end

**Checkpoint C: commands.** Spec success criteria 1–6 pass in the integration test.

### Phase 4: Finish
- T17 Hardening and docs

**Checkpoint D: done.** Success criteria of both specs (Slice A subset) met; `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo fmt --all -- --check` pass.

## Delivery

One branch per phase off `develop`, one commit per task, one PR per phase referencing the spec section it implements. Review at each checkpoint before starting the next phase.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Whole config model is larger than Slice A strictly needs | Med | Keep types plain; LSP merge and scope are small pure functions with the tests the spec lists; no extra features |
| DTO layer drifts from engine types | Med | Round-trip test with a config holding every optional block (workspace criterion 2) |
| `[tool.uv]` text match misdetects | Low | Fixture tests for uv, poetry, both, neither; documented ceiling; fails safe (ambiguous names the flag) |
| Relative-path computation edge cases (`..`, symlinks, different roots) | Med | Pure function with table tests; the adapter canonicalizes first; fall back to absolute |
| `project add` rollback leaves partial state | High | Use case order is memory first, config last; `discard(created)` on failure; fake-port tests inject a failure at each step |
| Hand-rolled temp dirs leak on test failure | Low | Unique names under the OS temp dir, removed in a drop guard |
| Existing `init` callers/tests break when rewritten | Low | Only `main.rs` calls it; the old unit test is replaced by T11's tests |

## Open questions

None blocking. Two choices above to confirm at plan review: (5) no TOML crate, and (7) no temp-dir crate. If you would rather approve `toml` and a temp-dir crate (`tempfile`), say so and T10 and T12 get simpler.
