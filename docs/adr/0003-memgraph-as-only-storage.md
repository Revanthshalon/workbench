# ADR-0003: Memgraph is the only storage, on the user's machine

## Status

Accepted (2026-11-01)

## Context

Workbench keeps a code graph, semantic annotations and cross-project relationships, queried by agents. It is one person's local tool: nothing listens on the network and nothing is shared.

## Decision

Use one storage, Memgraph, running on the person's own machine. Workbench never installs, starts or stops it; `developer-support` will not start until it answers. One database per workspace (a `namespace`), one graph per view. Agents query through a single read-only Cypher tool, `graph_query`, confined by an anchor guard to the namespace and view. Person-written annotations are also mirrored to `annotations.yml` so they can be replayed if Memgraph is rebuilt.

## Consequences

- Positive: one query language and one adapter; queries follow relationships across projects; no hosted service.
- Negative: the user must run Memgraph (mitigated by a start-up error with the `docker run` hint and `workbench storage status`). Agent-written annotations are lost if the database is wiped.
- Not supported: shared or team storage.

## Related

`docs/spec/SPEC-storage.md`; HLD §3 #4.
