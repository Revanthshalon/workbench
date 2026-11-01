# ADR-0002: Detect `pyproject.toml` managers by text match, not a TOML crate

## Status

Accepted (2026-11-01), subject to confirmation at Slice A plan review

## Context

`project add` must tell `uv` from `poetry` for Python projects. A new dependency needs approval, and `deny.toml` allows only MIT, Apache-2.0 and Unicode-3.0. The only question asked of `pyproject.toml` is whether a `[tool.uv]` or `[tool.poetry]` table exists.

## Decision

Match trimmed header lines (`[tool.uv]`, `[tool.uv.…]`, and the poetry equivalents) while reading the file. No `toml` dependency. The code carries a `ponytail:` comment naming the ceiling.

## Consequences

- Positive: no new dependency, licence review or compile cost.
- Negative: a header inside a multi-line string is a false positive. It fails safe: an ambiguous result makes the CLI name the flag to pass.
- Revisit: adopt a TOML parser if a later module needs real `pyproject.toml` contents, or if a false positive is ever reported. A new ADR supersedes this one.

## Related

[`tasks/plan.md`](../../tasks/plan.md) decision 5.
