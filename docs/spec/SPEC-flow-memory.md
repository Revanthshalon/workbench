# Spec: flow-memory

Module `flow-memory` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`storage`](SPEC-storage.md) (draft: the `FlowStore` port), [`audit`](SPEC-audit.md) (draft: the session log it reads), [`execution-flow`](SPEC-execution-flow.md) (draft: the flow model it stores) and [`semantic-layer`](SPEC-semantic-layer.md) (draft: node addresses). Used by `incidents` and `agent-prompts` (`explain_flow`, `write_rca`). `operational-support` only.

## Decisions already made elsewhere

1. **A flow is saved only after the user confirms the fix** (map). It is built on the session log (the audit trail) and on `execution-flow` results. `flow-memory` never calls a production system itself (map, Boundaries).
2. **Two copies, one working store.** The storage backend is the working store, used to query and combine flows. On confirmation **one file per flow** is also written to `.workbench/flows/`, and the **combined orchestration flow** is regenerated there (map).
3. `.workbench/` is git-ignored, so the files stay local; each person has their own Memgraph, so flows are **not shared across a team**.
4. Tools: save and recall a flow, the orchestration map, and flow files (to be defined). This spec defines them: `save_flow`, `retract_flow`, `recall_flow`, `orchestration_map`.
5. Confirmation goes through MCP **elicitation**, as a context switch does (`SPEC-contexts.md`); a client that cannot elicit cannot save a flow.
6. Secrets: resolved secrets are removed by exact match from everything written (`SPEC-audit.md`); nothing is scrubbed by pattern.

## Assumptions (correct me now)

1. **A flow's content comes from the log, not from the agent.** The agent supplies only the prose (a title, the symptom, the root cause, the fix) and *which* results to keep: path ids (`path_…`) and commit hashes. The hops, signatures, counts and commits are read from the `error_paths` and `recent_changes` results recorded in the session log (`tool_result` lines). An id that is not in the session's recorded results is refused, so an agent cannot make up a flow. A result the log cut at 64 KB cannot be used; the save says which ids it could not find.
2. **Raw data is never stored in a flow.** A flow holds normalised signatures (with placeholders), code hops (project, path, qualified name), service names, commit hashes and subjects, counts and times, and the confirmed prose. It never holds raw log lines, timeline text, database rows, Redis values or Kafka messages. The session log keeps the originals and the flow points to them (`session`, `seq` range).
3. **A flow is anchored like an annotation:** each hop's node is stored as an address (project, path, qualified name), not a view-specific id (`SPEC-semantic-layer.md`), so it can be found on any view. When a flow is read, hops whose node no longer exists in the view are listed as `missing_nodes`; nothing is dropped.
4. **A saved flow is immutable.** To correct one, retract it and save a new one. (Open Question 3.)
5. **Retracting needs the user's confirmation too**, by elicitation, and removes the flow from the storage and the file; the audit log keeps the record that it existed.
6. **The two copies are reconciled when an operational server starts** and after every save and retract: a flow in the storage without a file gets its file written; a flow file whose id is not in the storage is imported (the file is the recovery copy, as the mirror is for annotations). An id in both is never overwritten from the file, and a retracted flow's file is gone, so nothing comes back (Open Question 4).
7. **The orchestration map is computed from the stored flows**, not from the files, and is regenerated as a whole after each save and retract. It is deterministic (sorted), so its file changes only when the flows do.
8. **All contexts are combined**, each edge tagged with the contexts it was seen in; `orchestration_map` can filter by context (Open Question 5).
9. **Limits:** title 120 characters; symptom, root cause and fix 2 000 each; at most 10 paths and 20 commits per flow; `recall_flow` 10 results (50 at most).
10. **Flow ids** are `flow_` plus eight random hexadecimal characters and never change.

## Objective

Make every confirmed fix a reusable piece of knowledge about how the system fails and how its services really talk to each other, grounded in what the tools observed, and findable the next time something looks the same.

The module is done when:
- a flow can be saved only after an explicit user confirmation, and contains only content the log shows the tools returned,
- it is stored in the storage and written as a readable file, and the combined orchestration flow is regenerated,
- past flows can be recalled by text, service, code location or similarity to a current failure,
- the files can rebuild an empty storage, and a retraction leaves nothing behind but the audit record.

## The model

```text
Flow { id, title, context, session, saved_at, confirmed_via ("elicitation"),
       services[], symptom, root_cause, fix,
       paths[] (FlowPath from SPEC-execution-flow.md, hops anchored by address),
       changes[] ({commit, subject}),
       evidence[] ({session, seq_from, seq_to}) }
```

### The flow file

`.workbench/flows/<YYYYMMDD>-<slug>-<flow-id>.md`: YAML front matter with the whole model, then a readable body. Only the front matter is read back.

```markdown
---
id: flow_3f9a1c2e
title: Invoice creation fails when the stock service times out
context: staging
session: 5b0c4a1e
saved_at: 2026-10-30T11:02:00Z
confirmed_via: elicitation
services: [billing-api, stock-service]
symptom: POST /invoices returned 500 for about 12 minutes
root_cause: stock-service reserve calls exceeded the 3 s client timeout after a deploy removed its cache
fix: Restored the cache and raised the client timeout to 10 s (commit 4be19d0)
paths:
  - id: path_9b1c40a7
    signature: "TimeoutError: stock reserve after <n>s"
    count: 212
    first_seen: 2026-10-30T10:41:10Z
    last_seen: 2026-10-30T10:53:02Z
    hops:
      - {service: billing-api, role: entry, node: {project: billing-api, path: app/routes.py, symbol: create_invoice}, evidence: stack, confidence: resolved}
      - {service: billing-api, role: http-call, node: {project: billing-api, path: app/stock.py, symbol: StockClient.reserve}, evidence: stack, confidence: resolved}
      - {service: stock-service, role: entry, node: {project: stock-service, path: app/api.py, symbol: reserve}, evidence: graph, confidence: inferred}
changes:
  - {commit: 4be19d0, subject: Raise stock client timeout}
evidence:
  - {session: 5b0c4a1e, seq_from: 7, seq_to: 31}
---

# Invoice creation fails when the stock service times out

**Symptom.** …  **Root cause.** …  **Fix.** …

**Path `path_9b1c40a7`** (212 occurrences): `create_invoice` → `StockClient.reserve` → stock-service `reserve`
```

### The orchestration flow

`.workbench/flows/ORCHESTRATION.md`, regenerated from all stored flows:

- **Services** seen in flows, with the number of flows that touched each.
- **Edges** between services: where a path's consecutive hops cross a service boundary (an `http-call` hop followed by an entry hop in another service). Each edge has the count of distinct flows that support it, the contexts it was seen in, and up to 3 example flow ids.
- **Entry points** seen (routes, consumers, components) with their counts.
- **Fragile nodes:** code nodes that appear in the most flows.
- **Not yet observed:** links in the graph's overview (`SPEC-graph.md`) that no flow has crossed, so people can see what the map does not cover.
- A diagram in Mermaid text, and the same data as a table; both sorted so the output is stable.

## The tools

| Tool (kind) | Input | Answer |
|---|---|---|
| `save_flow` (`record`) | `title`, `symptom`, `root_cause`, `fix`, `paths` (path ids), `commits` (hashes, optional), `session` (default: this one) | Shows the user the draft for confirmation. On confirmation: the new id, the file path, and the services and edges the orchestration gained. Refused when the user declines or the client cannot elicit, when a path id or commit is not in the session's recorded results (naming it), or when a result was cut in the log |
| `retract_flow` (`record`) | `id` | Asks the user to confirm; then removes the flow and its file and regenerates the orchestration |
| `recall_flow` (`observe`) | `id`, or any of `query`, `service`, `node` (a project, file or symbol), `signature`, `similar_to` (a path id or a signature), `context`, `since`; `limit` | Matching flows, newest first, or with `similar_to` ranked by similarity; with `id` the whole flow. Each result lists `missing_nodes` for the view being read |
| `orchestration_map` (`observe`) | `context`, `projects`, `format` (`summary` default, or `mermaid`), `min_flows` | The services, edges, entry points, fragile nodes and not-yet-observed links described above |

### The confirmation

The elicitation shows the user: the title, symptom, root cause and fix as the agent wrote them, the services involved, the number of paths and commits kept, and the session. It offers an explicit statement to accept: **"I confirm this fix resolved the problem."** Only accepting it saves anything. The text the user sees is exactly what is saved, and the user may edit the title. Declined, cancelled or unsupported: nothing is stored or written, and the tool says so.

### Similarity

`similar_to` is deterministic: flows with the same normalised signature rank first, then by how many hop nodes (by address) they share with the given path (the share of the union, at least 0.3). No model is involved, and each result says which rule matched.

## Save, step by step

```text
read the session log (own session; another only if the audit module allows it)
  → find the tool_result lines for execution-flow tools; find each path id and commit hash asked for
       any missing or cut → refuse, naming them
  → build paths and changes from those results; flow services = the services on the hops
  → draft the prose from the agent's text; apply the limits; exact-match scrub of resolved secrets
  → elicit confirmation (the text above); not confirmed → stop, nothing written
  → one storage write (FlowStore); then write the flow file by temp file and rename, under the flows lock
  → regenerate ORCHESTRATION.md from the storage
  → answer: id, file, what changed in the orchestration
```

If the file write fails after the storage write, the tool answers that the flow is saved in the storage and names the file error; the next reconcile writes the file (Assumption 6).

## Tech Stack

- Rust 2024, `tokio`, `serde`, `yaml_serde` (already used), `thiserror`.
- No new dependencies.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Memgraph: cargo test --workspace -- --ignored    # the shared FlowStore suite against a local Memgraph
```

## Project Structure

```
lib/engine/src/flow_memory/
├── model.rs        # Flow, Evidence, ids, limits
├── build.rs        # building a Flow from logged results and the agent's prose
├── log.rs          # finding execution-flow results and path ids in a session log's events
├── similar.rs      # the deterministic similarity rules
├── recall.rs       # filters, missing nodes against a view
├── orchestration.rs # services, edges, entry points, fragile nodes, not-yet-observed, the Mermaid text
├── file.rs         # the flow file: front matter, body, file name, parsing it back
├── reconcile.rs    # the storage ↔ files plan (what to write, what to import)
└── ports.rs        # FlowFiles (read/write flow files and ORCHESTRATION.md under a lock), SessionLog (read events)

lib/engine/src/storage/ports.rs   # FlowStore: put_flow, get_flow, list_flows(filter), delete_flow

lib/adapters/src/flow_memory/files.rs   # `.workbench/flows/` with temp file, rename, owner-only permissions, the flows lock

services/cli/src/mcp/modules/flow_memory.rs   # the four tools
```

The engine has no I/O: building, similarity, recall, orchestration and reconcile are plain logic over `FlowStore`, `FlowFiles` and `SessionLog`.

## Code Style

```rust
/// What the user must have confirmed before a flow exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    pub confirmed: bool,
    pub title: String,
}
```

Follow existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **Confirmation:** declined, cancelled and an unsupported client each store and write nothing; accepting saves exactly the text shown; an edited title is kept.
- **Grounding:** a path id or commit that is not in the logged results is refused naming it; a result cut in the log is refused naming it; the saved hops equal the logged ones; the agent's text cannot add a hop.
- **No raw data:** a fixture session with raw log lines, DB rows and Kafka messages in its results produces a flow with none of them (searching the whole stored flow and file); a planted fake secret in the agent's prose is removed by the exact-match scrub.
- **File:** a golden file for the format above; round trip through the parser; the body is ignored on read; owner-only permissions; temp file and rename leave the old file intact on failure.
- **Reconcile:** a flow without a file gets one; a file whose id is not stored is imported; an id in both is not overwritten; a retracted flow's file does not return; a file with a bad front matter is reported and skipped, never edited.
- **Retract:** needs confirmation; removes storage and file; regenerates the orchestration; the audit log still has the save.
- **Recall:** each filter, the limit and its notice; `similar_to` ranks by the rules and names the rule that matched; `missing_nodes` appears for a view lacking a node and the flow is still returned.
- **Orchestration:** edges only where a path crosses a service boundary; counts are distinct flows; contexts tagged and filterable; fragile nodes ranked; not-yet-observed links come from the graph's overview; output is byte-identical on a second run with the same flows.
- **Shared suite:** every `FlowStore` operation against the in-memory fake and Memgraph.
- **Never calls production:** a fake remote runner and a fake datastore client record zero calls during every flow-memory tool.
- Every tool and every file or orchestration rule has a test.

## Boundaries

- **Always:** require the user's confirmation to save or retract; take hops, signatures and commits only from the session log; store addresses, not raw data; write files by temp file and rename; keep the audit record of saves and retractions.
- **Ask first:** letting an agent supply hops or text beyond the prose; storing raw log lines or data; editing a saved flow in place; sharing flows between people; deleting flow files other than by `retract_flow`.
- **Never:** save without confirmation; store a secret, a raw log line, a database row or a message; overwrite a stored flow from its file; call a production system or a datastore; change a person's flow file by hand-written edits (a bad file is reported, not repaired).

## Success Criteria

1. A flow is saved only after the user accepts the elicitation, and a client that cannot elicit cannot save.
2. Every hop, signature and commit in a saved flow was returned by a tool in the session's log; anything else is refused by name.
3. A saved flow holds no raw log line, database row, Redis value or Kafka message, and no secret.
4. Each flow exists in the storage and as a file; the file parses back to the same flow; `ORCHESTRATION.md` is regenerated and byte-stable.
5. `recall_flow` finds flows by text, service, node, signature and similarity, with the matching rule named and `missing_nodes` reported.
6. After the storage is wiped, starting an operational server restores every flow that still has a file, and a retracted flow does not return.
7. `orchestration_map` returns services, edges with supporting flow counts and contexts, entry points, fragile nodes and not-yet-observed links.
8. No flow-memory tool makes a remote or datastore call.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Content only from the log.** The agent can supply prose and choose which recorded results to keep, but not add hops. Too strict, for example when the fix was found by reading code rather than by a tool?
2. **No raw data in flows.** Signatures, addresses and commit hashes only, with pointers to the session log. Enough to be useful later?
3. **Immutable flows.** Correct by retract and save again. Or allow a revise with its own confirmation?
4. **Reconcile in both directions at server start.** Right, or should restoring from files be explicit?
5. **One combined map for all contexts**, each edge tagged with its contexts. Or one map per context?
6. **Elicitation is required.** A client that cannot elicit can recall flows but never save one, as with context switches. Accept?
