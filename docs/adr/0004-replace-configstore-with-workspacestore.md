# ADR-0004: Replace `ConfigStore` and `DirFileStore` with `WorkspaceStore`, cut over in T12

## Status

Accepted (2026-11-01)

## Context

The scaffold has a port `ConfigStore::create`, a use case `init(store, platforms)`, and an adapter `DirFileStore` that writes an **empty** `config.yml`. Slice A needs `init` to write a valid workspace config (storage block, generated namespace) and `project add | remove` to load and save the full config without losing hand-written blocks. `--platform` also leaves `init` (it belongs to `platform-setup`). The old port, use case and adapter cannot express any of this.

## Decision

- Introduce `WorkspaceStore` (`load`, `save`, `create_empty(&config)`), implemented by the YAML adapter (T7), and a new `init_workspace` use case (T11).
- T1 moves `DirFileStore` into the new `lib/adapters` crate unchanged. This proves the engine → adapters → cli wiring with a working adapter and keeps `init --out ./tmp` green.
- The new ports and use case are added **beside** the old ones (T6, T11), so every task from T1 to T11 builds and passes.
- **T12 is the single cutover:** `main.rs` switches to `init_workspace` and `WorkspaceFile`, and the same task deletes `ConfigStore`, the old `init`, `DirFileStore` (`dir_file.rs`) and the old unit test. `DirFileStore` is deleted because its only job, creating an empty file, is superseded by `WorkspaceStore::create_empty`, and nothing else uses it.

## Consequences

- Positive: no half-migrated state and no dead code after T12; the CLI keeps working at every checkpoint.
- Negative: `DirFileStore` is moved in T1 and deleted in T12, which is churn. The alternative, creating an empty `lib/adapters` in T1 and leaving `DirFileStore` in the CLI until T12, saves the move but leaves the new crate unexercised until T7.
- Risk: the old and new ports coexist for several tasks. T12 carries the deletion list so none is left behind.

## Related

[`tasks/plan.md`](../../tasks/plan.md) decision 3 and the risk "Existing `init` callers/tests break"; `tasks/todo.md` T1, T6, T11, T12; ADR-0001.
