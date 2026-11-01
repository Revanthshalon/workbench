# ADR-0001: Keep `lib/engine` free of `serde`

## Status

Accepted (2026-11-01)

## Context

The engine is the hexagonal core: domain types, ports and use cases, no I/O. The workspace spec makes `lib/adapters` the crate that depends on `serde` and `yaml_serde`. Reading `config.yml` also needs strict parsing: unknown keys must be rejected (`deny_unknown_fields`), which is a concern of the file format, not of the domain.

## Decision

Serialization lives in adapters. The YAML adapter owns serde DTOs (`deny_unknown_fields`) and maps them to engine types, running engine validation on the result. The engine depends on `thiserror` only.

## Consequences

- Positive: the engine stays format-agnostic and testable with fake ports; another file format or transport needs only a new adapter.
- Negative: one DTO layer that can drift from the engine types. Mitigation: a round-trip test over a config holding every optional block (workspace success criterion 2).
- Rule for later modules: engine types do not derive `Serialize`/`Deserialize`.

## Related

[`tasks/plan.md`](../../tasks/plan.md) decision 1; HLD §3 #2.
