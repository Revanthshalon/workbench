# Spec: execution-flow

Module `execution-flow` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`graph`](SPEC-graph.md) (draft: callers, routes, cross-project calls, changed symbols), [`operations`](SPEC-operations.md) (draft: logs, stack traces, workload state) and [`contexts`](SPEC-contexts.md) (approved: the active context and its branch view). Used by `flow-memory` (which stores what this module returns) and `agent-prompts`. `operational-support` only.

## Decisions already made elsewhere

1. Tools: `error_paths`, `request_timeline` and `recent_changes` (kind `observe`). The map also lists *blast radius* here; it is already served by `service_map` with `focus` (`SPEC-graph.md`) and is not duplicated.
2. **No new source of truth.** This module composes what `graph`, `operations` and git already give. It calls no LLM and changes nothing.
3. In `operational-support` code is read from the **active context's branch view**, and `recent_changes` reads the history of the context's branch (`SPEC-contexts.md`). Every answer names its context.
4. Remote work goes only through `operations`' catalog and the remote runner (`SPEC-operations.md`); each command is an audit event. This module adds **no** command templates.
5. `flow-memory` builds on the results of this module and on the session log; it saves a flow only after the user confirms the fix (`SPEC-flow-memory.md`).
6. Nothing is scrubbed by pattern; resolved secrets are removed from output by exact match (`SPEC-audit.md`).

## Assumptions (correct me now)

1. **Evidence, then graph, then a labelled guess.** A stack trace is the strongest evidence of a failing path (it is what actually ran). The graph extends it where logs end, with `confidence` on every hop (`SPEC-graph.md`). Nothing is presented as a cause: a path says *where* and *through what*, never *why*.
2. **The failing frame is the innermost frame that belongs to a project file** in the context's view (a Python traceback's last project entry; the first project entry of a Node stack). Frames in libraries stay in the text and are not hops.
3. **An entry point** is a node with a `HANDLES` edge (a route handler), a Kafka or queue consumer function recognised by the graph, an Angular component, or a node with no callers in the view. Walking up from the failing frame stops at the first entry point and at 8 hops.
4. **Cross-service following uses two kinds of evidence:** a `CALLS_HTTP` edge from a hop to a route in another project (graph), and errors found in that other service's logs within 60 seconds of the first occurrence (operations). It is on by default for at most 3 services and 2 service-hops, which costs at most that many extra log fetches (Open Question 1).
5. **Paths rank by a fixed rule:** new in the window first (not seen in the window just before), then occurrence count, then last seen. There is no score from a model.
6. **Next steps are suggestions, not actions.** A path may carry `next_steps`: tool names with arguments and a fixed-rule reason (for example a signature mentioning a connection timeout suggests `datastore_health`). The module never calls them.
7. **A request id is a literal.** `request_timeline` takes a string of 4 to 128 characters from `[A-Za-z0-9_.:-]` and searches logs for it as a case-sensitive substring. It does not know your logging format. With `expand: true` it does one more round: ids found next to the keys `trace_id`, `traceId`, `request_id`, `requestId`, `correlation_id`, `correlationId` and `x-request-id` in matched lines (at most 5) are searched too.
8. **Order comes from the platform's timestamps** (`kubectl logs --timestamps`, Compose `--timestamps`), not from the application's own. Events from different pods are merged by time; equal times keep each pod's own order. Clock differences between nodes are possible and the answer says it merges by platform time.
9. **Which services:** `request_timeline` searches the services named in `services`; if none are named it uses the namespace's Deployments (or the Compose services) when there are at most 12, and refuses with the list when there are more.
10. **`recent_changes` is the history of the branch at its last sync**, read with read-only git commands in the context's worktree. Workbench does not know what is actually deployed. When the running image's tag contains a commit hash that is in the branch's history (`SPEC-operations.md`, `check_config`), the answer marks commits after it `not_in_running_image`; otherwise `deployed: unknown`. That marker is labelled a heuristic.
11. **Author names appear, emails never do.** Commit messages are returned as the first line plus the body capped at 500 characters.
12. **Limits (defaults, maximums):** `error_paths` 5 paths (10), 30 hops per path; `request_timeline` 200 events (500); `recent_changes` 30 commits (100), 30 files and 50 changed symbols per commit; windows at most 24 hours for logs and 90 days for history.
13. Stable ids: a path's id is `path_` plus eight hexadecimal characters of the SHA-256 of its service, signature and node ids, so the same failure keeps the same id across runs and `flow-memory` can refer to it.
14. As in `SPEC-operations.md`, the map's dependency on `lsp` is not used here; frames are mapped with the graph only.

## Objective

Turn "something is failing" into evidence an agent can reason from: where in the code it fails and how the request got there, what one request did across services and when, and what changed in the deployed code just before.

The module is done when:
- `error_paths` returns ranked failure paths from real stack traces, extended through the graph and across services, each hop with its evidence and confidence,
- `request_timeline` merges one request's log lines across services into an ordered timeline with gaps and the services that never saw it,
- `recent_changes` lists the branch's recent commits with the symbols they touched, using read-only git,
- results are in the shared flow model, with stable ids, so `flow-memory` can store them,
- every remote command and git call is bounded, read-only and visible in the session log.

## The flow model

Defined here and used by `flow-memory`:

```text
Hop        { service, node? {id, kind, path, line}, role (entry | frame | http-call | external),
             evidence (stack | graph | log), confidence (resolved | inferred | unresolved), text? }
FlowPath   { id, signature, hops[], count, first_seen, last_seen, is_new, level, next_steps[] }
TimelineEvent { time, service, pod, level, text, matched (the id that matched) }
Timeline   { ids_used[], events[], gaps[], first_error?, not_seen[], merged_by: "platform time" }
Change     { commit, author, date, subject, body?, files[], symbols[], not_in_running_image? }
```

Every result also carries `context`, `view`, the window and the time it was made.

## The tools

| Tool | Input | Answer |
|---|---|---|
| `error_paths` | `service`, `since` (default 15m), `signature` or `error` (optional: focus on one message), `follow` (default true), `max_paths` | Ranked `FlowPath`s, then the services that were followed and what was found there, then the findings of the workloads involved (`SPEC-operations.md`) |
| `request_timeline` | `id`, `services`, `since` (default 1h), `expand` (default false) | A `Timeline`: ordered events, the largest gaps, the first error, the services that never saw the id, and the ids used |
| `recent_changes` | `since` (default 7d, or an ISO date), `projects`, `service`, `path`, `around` (a time to mark commits before and after), `limit`, `with_symbols` (default true) | `Change`s newest first with the branch, the commit that is the branch's head and where it was synced |

A call with an unknown service lists the known ones. A system that is not configured for the context says so. If the SSH connection fails the tool fails naming the context and host.

### `error_paths`

```text
fetch the window for the service (operations: logs, signatures, stack traces, workload state)
  → pick the error signatures (the one asked for, else the top ones by the ranking rule)
  → per signature, take its stack traces
       failing frame = innermost project frame; frames outermost → innermost are the hops
       frames map to nodes in the context's view (graph: nodes at those lines)
  → extend up: callers from the graph until an entry point (≤ 8 hops), confidence from the edges
  → follow across services (if enabled): CALLS_HTTP edges from hops, and errors in those services
       within ±60 s of the first occurrence; same procedure there; ≤ 3 services, ≤ 2 service-hops
  → attach findings, next_steps (fixed rules), the path id
  → rank (Assumption 5) and cut to max_paths
```

A signature with no stack trace (a plain error line) still returns a path of one hop if its message names a file and line that map to a node; otherwise a `FlowPath` with no hops and its counts, so the evidence is not lost.

### `request_timeline`

```text
for each service (≤ 4 fetches at a time, per the context's remote limit):
    logs for the window, platform timestamps, lines containing the id
  → expand once, if asked (Assumption 7)
  → merge by time; mark gaps (the 3 largest between consecutive events, with the services on each side)
  → first_error = the first event at level ERROR or above
  → not_seen = services searched with no match
```

### `recent_changes`

```text
the context's worktree for each project in scope (created by `context sync`; missing → fail with the fix)
  → read-only git in the worktree: log of the branch's head within the window
  → per commit: author name, date, subject, body (capped), files (cut), and when with_symbols:
       the diff's changed line ranges mapped to nodes in the view (graph: changed_symbols)
  → if the running image's commit is known (Assumption 10), mark commits after it
```

**Read-only git:** the adapter runs only `git log`, `git diff-tree` and `git diff` (with `--no-color`, `-U0`), never anything else, with every revision matched against `[0-9a-f]{7,40}`, every path relative with no `..` and passed after `--`, and no argument starting with `-`. It never fetches, checks out or writes.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `serde_json`, `thiserror`.
- No new dependencies. `git` is used as a program, as in `SPEC-contexts.md`.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Fixtures: cargo test -p engine execution_flow
```

## Project Structure

```
lib/engine/src/execution_flow/
├── model.rs        # Hop, FlowPath, Timeline, TimelineEvent, Change, ids
├── error_paths.rs  # selecting signatures, building paths from traces, extending through the graph, following services
├── entry.rs        # what counts as an entry point
├── rank.rs         # the fixed ranking rule
├── hints.rs        # next_steps from fixed rules
├── timeline.rs     # id search, expansion, merging, gaps
└── changes.rs      # history window, mapping diffs to nodes, the running-image marker

lib/engine/src/ports.rs   # + HistoryReader (read-only git history of a worktree)

lib/adapters/src/git_history.rs   # HistoryReader over the git program, the allowed subcommands and argument rules

services/cli/src/mcp/modules/execution_flow.rs   # the three tools
lib/engine/tests/fixtures/execution_flow/        # two-service logs, traces, graph partitions, a git repository builder
```

The engine has no I/O: it uses `operations`' parsing and the graph through their ports, and `HistoryReader`.

## Code Style

```rust
/// One step of a failure path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub service: String,
    pub node: Option<NodeRef>,
    pub role: Role,
    pub evidence: Evidence,
    pub confidence: Confidence,
    pub text: Option<String>,
}
```

Follow existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine.

## Testing Strategy

- **Paths from traces:** a Python traceback and a Node stack each give the right hops and failing frame; library frames are not hops; frames map to nodes in the context's view and never in `working`.
- **Extending:** callers are followed up to an entry point and stop at 8 hops; a cycle terminates; every hop carries the edge's confidence.
- **Following across services:** a `CALLS_HTTP` edge to another project fetches that service's window around the first occurrence; the 3-service and 2-hop caps hold; `follow: false` fetches nothing extra.
- **Ranking and ids:** new before frequent before recent; the same fixture gives the same ids; a different signature gives a different id.
- **Hints:** each fixed rule produces its `next_steps`; none is executed.
- **No trace:** a plain error line maps through its file and line, else yields a no-hop path with counts.
- **Timeline:** a two-service fixture merges by platform time with ties in per-pod order; gaps are the three largest; `first_error` is right; a service with no match is in `not_seen`; `expand` finds the extra ids once and no more than 5; ids that break the pattern are refused; more than 12 default services is refused with the list.
- **`recent_changes`** (a real `git` and a temporary repository): the window, `path` and `service` filters, cherry-picked commits on the release branch, changed symbols mapped to nodes, files and symbols cut at their limits, `around` marking, the running-image marker for a known and an unknown tag; emails never appear.
- **Read-only git:** a fake `git` records every call; only `log`, `diff-tree` and `diff` occur; injection attempts (a leading `-`, `..` in a path, a non-hex revision, shell characters) are refused before any call; no network call happens.
- **Limits** for every tool, and cut notices.
- **Audit:** every fetch is a `remote_command` event, each git call is recorded under the tool's `call_id` (`SPEC-audit.md`).
- **Failures:** unknown service lists the known ones; no worktree fails with the fix; an SSH failure names the context and host.
- Every tool and every row of the flow model has a test.

## Boundaries

- **Always:** use stack traces as primary evidence and label graph-derived hops with their confidence; say which context and view answered; bound fetches, hops, events and commits and say when cut; run git only through the read-only adapter.
- **Ask first:** following more services or hops; running any `next_steps` automatically; adding a git subcommand; including author emails; searching more than 12 services at once; calling an LLM.
- **Never:** present a path as a cause; change an observed system or a repository; fetch, check out or write in git; guess a request id format; return an email address; add a command template (they belong to `operations`).

## Success Criteria

1. `error_paths` returns ranked paths whose hops come from real stack frames mapped to nodes in the context's branch view, extended through the graph with confidence, and across at most 3 services.
2. Ranking is the fixed rule, ids are stable across runs, and `next_steps` are suggestions that are never run.
3. `request_timeline` returns one ordered timeline across the searched services with gaps, the first error and the services that did not see the id.
4. `recent_changes` lists the branch's commits in the window with the nodes they touched, using only `git log`, `git diff-tree` and `git diff` in the context's worktree, and never shows an email.
5. The running-image marker is `unknown` unless the image tag contains a commit hash in the branch's history.
6. Every answer names its context and view and is bounded with a cut notice.
7. Every remote command and git call is in the session log.
8. The results use the flow model, so `flow-memory` can store them unchanged.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Following across services by default.** It costs up to three extra log fetches against the live environment per call, each bounded by the runner's limits. On by default, or opt-in?
2. **Which services a timeline searches** when none are named: all Deployments up to 12, or always require a list?
3. **`recent_changes` shows the branch at its last sync**, not proven to be what runs. Is the image-tag marker (labelled a heuristic) enough?
4. **Author names are shown, emails are not.** Right line for commit authors?
5. **`next_steps`** are suggestions only, from fixed rules. Useful, or noise to drop?
6. **Request ids** are searched as literals with an optional one-round expansion over six known key names. Enough for your logging, or should the key names be configurable?
