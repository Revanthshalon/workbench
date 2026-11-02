# Implementation Plan: Slice A (workspace model + `init`, `project`, discovery)

Specs: [`SPEC-workspace.md`](../docs/spec/SPEC-workspace.md) and Slice A of [`SPEC-cli.md`](../docs/spec/SPEC-cli.md) (both approved). Task details and acceptance criteria are in [`todo.md`](todo.md). Design: [HLD](../docs/hld/HLD-workbench.md) (system) and [LLD](../docs/lld/LLD-workspace-cli.md) (types, ports, algorithms for this slice). Decisions: [ADRs](../docs/adr/README.md).

## Overview

Greenfield build. Nothing exists yet except the root `Cargo.toml` (workspace members, pinned dependency versions, lints), the licence and the docs. Build the three crates, the whole `workspace` module (model, validation, YAML store, memory layout, scope, LSP precedence) and, on top of it, the CLI commands `workbench init`, `project add | list | remove`, workspace discovery (`--workspace`, search upward) and the common error behaviour. Nothing here needs Memgraph, `core`, or the `index`/`semantic` modules.

**Out of Slice A** (per the CLI spec): the `.workbench/mcp/` templates written by `init`, `--index` on `project add` (needs `index`), and the `mcp`, `storage status`, `index`, `watch`, `semantic`, `context`, `platform` groups. `init` has no `--platform`.

## Crates

| Crate | Path | Depends on | Holds |
|---|---|---|---|
| `engine` | `lib/engine` | `thiserror` | domain types, ports, use cases; no I/O |
| `adapters` | `lib/adapters` | `engine`, `serde`, `yaml_serde` | driven adapters: YAML config, memory folders, gitignore, project inspection, ids |
| `workbench` | `services/cli` | `engine`, `adapters`, `clap` | clap commands, output formatting, wiring |

## Architecture decisions

1. **Engine stays free of `serde`** ([ADR-0001](../docs/adr/0001-engine-free-of-serde.md)). The YAML adapter owns serde DTOs (`deny_unknown_fields`) and maps them to engine types. Cost: one DTO layer.
2. **Whole config model from the start, not just what Slice A edits.** `project add` must save a config that may hold hand-written `lsp`, `compose` and `semantic.llm` blocks without losing them, and workspace success criterion 2 requires the round trip. So the model carries every field, including `framework` and hand-written `binaries` (never detected, so no new parser; see the [design](../docs/superpowers/specs/2026-11-02-project-language-framework-binaries-design.md)).
3. **Randomness without a new dependency:** `IdSource` is implemented in `lib/adapters` from std (`RandomState`); the engine takes the port, tests inject a fixed suffix.
4. **`pyproject.toml` detection without a TOML crate** ([ADR-0002](../docs/adr/0002-no-toml-crate-for-pyproject.md)): match trimmed `[tool.uv…]` / `[tool.poetry…]` header lines. Mark with a `ponytail:` comment.
5. **Relative path storage:** `project add` stores the path relative to the workspace root (using `..`), or absolute when no relative form exists. The computation is a pure function in the engine over absolute, already-resolved paths; the CLI canonicalizes first.
6. **The engine does no I/O, so `project add` checks the directory and a given `--compose` file through `ProjectInspector::inspect(path, compose)`.** `ContextSource` returns `SourceFile { name, content, truncated }`; the adapter cuts at 20 KB and the engine's seed builder adds the `[truncated at 20 KB]` marker.
7. **Integration tests** use `env!("CARGO_BIN_EXE_workbench")` and `std::process::Command`, each in a fresh directory built from `std::env::temp_dir()` plus a unique name (no temp-dir crate, which is ask-first). A helper of a few lines in `tests/cli.rs` creates and removes it; the adapter tests carry their own copy under `#[cfg(test)]`. The engine's recording fakes live in one `#[cfg(test)]` `testing` module created in T6.
8. **`project remove --purge-memory`** prints what it will delete and then deletes (no prompt), as the spec says.

## Dependency graph

```
T1 scaffold: engine, adapters, workbench build clean
 └─ T2 ProjectId, Language, DependencyManager ── T3 Project, Environment, LSP merge ── T4 WorkspaceConfig + validation ── T5 Scope
                                                                                      └─ T6 memory layout + ports + fakes   (needs T2, T4)
T6 ─┬─ T7 YAML WorkspaceStore adapter           (needs T4, T6)
    ├─ T8 MemoryStore + ContextSource adapter   (needs T6)
    ├─ T9 Gitignore + IdSource adapters         (needs T6)
    └─ T10 ProjectInspector adapter             (needs T2, T6)

T11 init use case (engine)                      (needs T4, T6)
T12 `workbench init` end to end                 (needs T1, T7, T8, T9, T11)
T13 discovery + `project list`                  (needs T7, T12)
T14 add_project use case                        (needs T4, T6)
T15 `project add` end to end                    (needs T13, T14, T8, T10)
T16 `project remove` end to end                 (needs T13, T8)
T17 hardening and docs                          (needs all)
```

The engine chain T2 → T3 → T4 → T6 is sequential; T5 can run beside T6. Once T6 exists, T7–T10 are independent of each other. T11 and T14 (engine only) can run beside the adapters. Everything else is sequential.

## Task index (details in `todo.md`)

### Phase 1: Foundation
- T1 Workspace scaffold
- T2 Project identifiers and enums
- T3 Project, environment and LSP precedence
- T4 Workspace config and validation
- T5 Project scope
- T6 Memory layout and ports

**Checkpoint A: foundation.** Engine tests pass, clippy clean, `workbench --help` runs.

### Phase 2: Adapters
- T7 YAML workspace store
- T8 Memory and context adapters
- T9 Gitignore and id adapters
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
| Relative-path computation edge cases (`..`, symlinks, different roots) | Med | Pure function with table tests; the CLI canonicalizes first; fall back to absolute |
| `project add` rollback leaves partial state | High | Use case order is memory first, config last; `discard(created)` on failure; fake-port tests inject a failure at each step |
| Hand-rolled temp dirs leak on test failure | Low | Unique names under the OS temp dir, removed in a drop guard |

## Open questions

None blocking. Decisions 4 (no TOML crate) and 7 (no temp-dir crate) are to confirm at plan review. If you would rather approve `toml` and `tempfile`, T10 and T12 get simpler and ADR-0002 is superseded.
