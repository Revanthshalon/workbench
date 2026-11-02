# Design: project `language`, `framework` and `binaries`

Status: **proposed.** Changes the approved [`SPEC-workspace`](../../spec/SPEC-workspace.md) and Slice A of [`SPEC-cli`](../../spec/SPEC-cli.md). Once approved, the specs, [LLD](../../lld/LLD-workspace-cli.md) and [`tasks/todo.md`](../../../tasks/todo.md) are updated to match; this document is not the source of truth after that.

## Intent

Two gaps in the workspace model:

1. `language: angular` names a framework, not a language. Parsing is by language (tree-sitter grammars: Python, TypeScript, HTML, CSS, SCSS, …), so Angular is TypeScript to the parser.
2. One repository can hold several runnable things (several Python entry points, several Angular apps). The model has no place for them.

## Goals / non-goals / anti-goals

- Goal: `language` says what the code is written in; `framework` says what it is built on.
- Goal: a project can list its binaries, kept inside the one project entry.
- Non-goal: detecting binaries. Nothing reads `pyproject.toml` or `angular.json` for them, so no parser dependency is added and ADR-0002 stands.
- Non-goal: a `project add` flag for binaries, a Compose service mapping per binary, memory or annotations per binary.
- Anti-goal: a second framework forcing a new `language` value.

## Approaches

1. **`language` plus optional `framework`** (chosen). Matches the grammar choice; a new framework is one new `framework` value.
2. Keep `language: angular`, only add `binaries`. Smallest diff, leaves the gap.
3. One `stack` field. Same mixing as today under another name.

## Decisions

```yaml
projects:
  - id: web
    path: ../shared/web
    language: typescript          # python | typescript
    framework: angular            # optional; angular only for now
    dependency_manager: pnpm
    binaries:                     # optional; hand-written, never detected
      - name: admin
        entry: projects/admin/src/main.ts
      - name: shop
        entry: projects/shop/src/main.ts
```

| Field | Rule |
|---|---|
| `language` | Required. `python` or `typescript`. |
| `framework` | Optional. `angular`, valid only with `typescript` (an error otherwise). |
| `dependency_manager` | Pairs with `language`: `poetry`/`uv` for `python`; `npm`/`pnpm`/`yarn`/`bun` for `typescript`. A mismatch is an error. |
| `environment.venv` | Python only, as today (now an error on any `typescript` project, not only Angular). |
| `binaries[].name` | Required; same grammar as a project id; unique within the project. |
| `binaries[].entry` | Required; a relative path from the project directory. Stored as written, never checked for existence (the config loads when paths are missing). |

- Top-level `lsp.<language>` is keyed by `python` or `typescript`. Choosing an Angular language server for `framework: angular` is left to the `lsp` spec.
- CLI: `--language <python|typescript>`; new `--framework <angular>`; no flag for binaries.
- Detection: `angular.json` present → `typescript` + `angular`. A `typescript` project without `angular.json` is not detected; the person passes `--language typescript`. The Python rules are unchanged.
- `project list` prints the language column as `typescript/angular` when a framework is set, `python` otherwise.
- Round trip: `binaries` and `framework` survive `project add`, `remove` and re-save like every other hand-written field.

## Constraints

Engine stays free of `serde` (ADR-0001). No new dependencies.

## Left to the builder

Error wording; the exact `BinaryName` type (reuse the id rules, own newtype); fixture names.

## Calls I made (change any)

- `entry` is required, relative, and unchecked.
- Binaries are config-only in Slice A.
- Compose `services` still map to a project id, not a binary.
- `typescript` without `angular.json` is not auto-detected.
- List column shows `typescript/angular`.

## What changes once approved

`SPEC-workspace` (model, rules table, success criteria, round trip), `SPEC-cli` (flags, detection table, examples, tests), capability map line 25, LLD (`Language`, new `Framework` and `Binary`, `Invalid` fields), `tasks/todo.md` T2, T3, T4, T10, T14, T15 and T17.
