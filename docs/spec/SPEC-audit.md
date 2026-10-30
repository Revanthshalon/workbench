# Spec: audit

Module `audit` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`core`](SPEC-core.md) (approved), which owns the session log files and the call hook this module uses. Written to by `core` (every tool call), [`contexts`](SPEC-contexts.md) (context switches, checks, remote commands, tunnels) and every other tool module. Read by `flow-memory`, by `workbench mcp logs`, and by two MCP tools defined here.

## Decisions already made elsewhere

1. The audit record is the **per-session log file**, `<logs.dir>/sessions/<session-id>.log`, in **`operational-support` only**. There is **no copy in the storage backend**. `developer-support` writes no audit record (git is the record of its edits).
2. The file is **JSON Lines**: one JSON object per line. The server **only appends** and never rewrites or deletes an event. `workbench mcp logs --session ID` renders it for people and `--raw` prints the lines as written.
3. A **session** is one connected client, identified by the token its shim generates. The token survives a server restart, so a session that continues across a `restart` keeps appending to the **same file**.
4. `logs.dir` is set in the server config (default `.workbench/logs`). The directory is created `0700` and every file `0600`.
5. **Credentials are never written.** Secrets are `secrecy` values that are never exposed outside the point where a connection is opened. There is no pattern-based redaction; everything else is recorded.
6. This spec defines the event format, the session boundary, retention, the rules for recording results, and the retrieval tools. `core` calls it through its call hook and its `AuditSink` port; `core` does not know the format.

## Assumptions (correct me now)

1. **Fail closed.** If an event cannot be written (disk full, permission lost, directory removed), the tool call is refused with `audit log unavailable: <reason>`, and nothing runs. Operational access to production without a record is not allowed.
2. **A result is recorded, capped.** Besides a short `summary`, a tool's structured result is recorded in the event, up to 64 KB. A larger result is cut, with `truncated: true`, its full size and a SHA-256 digest of the full result, so a reader can tell it is incomplete. (SPEC-core described only "a short result summary"; this spec amends that line.)
3. **One writer per session file.** Only the one `operational-support` server of a workspace writes sessions, and each session belongs to one connected shim at a time, so a file never has two writers.
4. **Times are RFC 3339 in UTC**, to the millisecond.
5. **Any session in the logs directory can be read** through the retrieval tools, not just the current one, because the files are the person's own and `flow-memory` needs earlier sessions. Reading is itself a tool call and is recorded.
6. The audit module registers only in `operational-support`.

## Objective

Give a person a complete, trustworthy account of what an agent did against their environments: which tools it called with what arguments, what came back, which context it was in, when it switched, and what failed. It must survive a server restart, be easy to read and to parse, and never contain a credential.

The module is done when:
- every tool call, context switch, remote command, tunnel and error in an operational session is a line in that session's file, in order, before the thing it describes takes effect,
- a session that spans a restart is one continuous, correctly numbered file,
- no resolved secret value appears in any line,
- a person can list and read sessions with `workbench mcp logs`, and an agent can read them with the retrieval tools,
- a call that cannot be recorded does not run.

### The file

`<logs.dir>/sessions/<session-id>.log`, one JSON object per line, UTF-8, ending in `\n`. The session id is the shim's token (a UUID).

Every line has the same envelope:

```json
{"v":1,"seq":14,"ts":"2026-10-30T09:12:44.318Z","session":"5b0c…","server":"server-operational-support-20261030-091200","event":"tool_call", "…": "…"}
```

| Field | Meaning |
|---|---|
| `v` | Format version, `1`. Readers ignore unknown fields. A change that breaks readers bumps it. |
| `seq` | Starts at 1 and rises by 1 for every line in the file, including across restarts: a server that resumes a session reads the last line and continues. |
| `ts` | When the event happened, RFC 3339 UTC. |
| `session` | The session id. |
| `server` | The id of the server process that wrote the line (the name of its server log), to match the two. |
| `event` | The event type. |
| `context` | The session's active context at that moment, on every event that has one. |

### Events

| Event | Written when | Fields beyond the envelope |
|---|---|---|
| `session_started` | The first time a token is seen | `mode`, `start_context`, `client` (name, version, whether it can elicit), `workbench_version` |
| `session_resumed` | A known token connects again (after a restart or a dropped connection) | `reason` (`reconnect` or `server_restart`) |
| `server_stopping` | The server stops or restarts while the session is open | `reason` (`stop` or `restart`) |
| `session_ended` | The client says goodbye, or does not return within the 90-second reconnect window | `reason` (`client_closed` or `reconnect_timeout`) |
| `tool_call` | A tool is invoked, **before** it runs | `call_id`, `tool`, `kind`, `arguments` |
| `tool_result` | The tool finishes | `call_id`, `ok`, `duration_ms`, `summary`, `result`, `truncated`, `size_bytes`, `sha256` (only when truncated), `error` (when not `ok`: `code`, `message`) |
| `remote_command` | A command ran on a VM (`SPEC-contexts.md`) | `call_id`, `program`, `arguments`, `exit_code`, `duration_ms`, `output_bytes` |
| `tunnel` | A tunnel opened or closed | `call_id`, `action` (`opened` or `closed`), `system`, `name`, `remote` (host and port as the VM sees them) |
| `context_check` | The context check ran (at start, or for a switch) | `target`, `results` (per system: `status`, `latency_ms`, `message`) |
| `context_switch` | A switch was asked for | `from`, `to`, `outcome` (`switched`, `declined`, `refused`), `reason` (when not `switched`) |
| `error` | Something failed that no tool call reports, for example a connection that went down | `where`, `message` |

- `tool_call` and `tool_result` are separate lines joined by `call_id`, so a call that never finished (a crash) is visible as a `tool_call` with no result.
- `remote_command`, `tunnel` and `context_check` that happen during a call carry its `call_id`.
- `arguments` and `result` are the JSON the tool received and returned, after the scrub below. The `graph_query` text is therefore in `arguments`, and its row count and time are in `summary`.
- The retrieval tools do not store the events they return in their own `result` (they would copy a whole log into itself); their `summary` says how many events were returned.

### What is never written, and what is cut

- **Resolved secret values.** Before a line is written, every string in it (arguments, results, messages, remote output) has every resolved secret value of every context in use removed by exact match and replaced with `[redacted]`. This is a safety net: secrets never reach the audit module in the first place, but text from a driver or from a command on a VM could contain one. It is not a pattern search.
- **Size.** A `result` over 64 KB is cut as described above. A `summary` is at most 2 KB. An `arguments` value over 64 KB is cut the same way. No line is longer than 160 KB.
- Everything else is kept as it is: query rows, log lines and file contents a tool returned stay in the record (up to the cap).

### The session boundary

- A session **starts** when a shim's token is seen for the first time (`session_started`).
- It **continues** when the same token connects again: after a dropped connection, or after a server restart (`session_resumed`). `seq` carries on.
- It **ends** when the shim says goodbye, or when no connection with its token has come back within the 90-second reconnect window (`session_ended`). If the server itself stops first, `server_stopping` is written to every open session, and `session_ended` follows when the window runs out (a stopped server cannot write it, so the next server that starts writes `session_ended` with reason `reconnect_timeout` for any open session file whose last event is older than the window, before it accepts connections).
- A session file that never received `session_ended` (a crash) is treated by readers as ended at its last event.

### Durability and failure

- Each event is written as one complete line, flushed and synced to disk before the call it describes proceeds (for `tool_call`) or before its result is returned (for `tool_result`).
- A reader tolerates a truncated last line (a crash mid-write) by ignoring it and saying so.
- If the line cannot be written, the call is refused (Assumption 1); if `session_ended` or `server_stopping` cannot be written, the failure goes to the server log.
- The directory and files are `0700` and `0600`. The files can hold production data, so they stay on this machine and are not copied anywhere.

### Retention

Nothing is deleted automatically and nothing is rewritten. Session files stay until a person removes them. `workbench mcp logs --sessions` shows each file's size so people can judge. A `--prune` command (delete or archive sessions older than a date) is on the roadmap; so is a hash chain that makes edits to a file detectable.

### Reading it

- **`workbench mcp logs --sessions`**: one line per session: id, start time, end time or `open`, start context, number of events, number of tool calls, file size.
- **`workbench mcp logs --session ID`**: the session rendered for reading, one line per event, oldest first, for example:

```text
09:12:44.318  tool_call    #7  fetch_logs  context=staging  {"service":"billing","since":"15m"}
09:12:44.342  remote_cmd   #7  kubectl logs billing-7d9f -n billing --since=15m  exit=0  412 KB
09:12:44.601  tool_result  #7  ok  283 ms  "412 lines from 1 pod"  (result cut at 64 KB of 412 KB)
09:13:02.007  context_switch   staging → production  declined
```

  `--raw` prints the JSON lines as written. Rendering never changes what is on disk.
- **MCP tools** (kind `observe`, `operational-support` only):

| Tool | Input | Result |
|---|---|---|
| `audit_sessions` | `limit` (default 20), `since` | The sessions as in the list above |
| `audit_events` | `session`, `from_seq` (default 1), `limit` (default 200, at most 1 000), `events` (a list of event types to keep) | The events from that point on, with `next_seq` so a reader can continue |

## Tech Stack

- Rust 2024, `serde` and `serde_json` (`serde` is already a workspace dependency), `thiserror`, `secrecy`.
- New dependencies, each needing approval: `serde_json`, a time library for RFC 3339 timestamps (for example `time`), and a SHA-256 implementation (for example `sha2`) for the digest of a cut result.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Read:     cargo run -p workbench -- mcp logs --sessions
          cargo run -p workbench -- mcp logs --session <ID>
          cargo run -p workbench -- mcp logs --session <ID> --raw
```

## Project Structure

```
lib/engine/src/audit/
├── event.rs       # AuditEvent (one variant per event above), the envelope, the version
├── scrub.rs       # exact-match removal of resolved secret values, from any JSON value
├── cap.rs         # the size caps: truncation, size_bytes and the digest
├── render.rs      # the human rendering of a session
└── summary.rs     # the session list: counts, start, end, context, size

lib/engine/src/ports.rs      # + AuditSink (record one event; fails if it cannot be recorded), AuditSource (list sessions, read events, from a sequence number)

lib/adapters/src/audit/
├── file_sink.rs   # appends one line per event, flushes and syncs, creates the directory and file with the right permissions, resumes the sequence number from the last line
└── file_source.rs # lists session files and reads events, tolerating a truncated last line

services/cli/src/mcp/
├── audit_hook.rs        # the call hook: writes tool_call before and tool_result after, refuses the call if it cannot record
└── modules/audit.rs     # audit_sessions and audit_events
```

`AuditSink` is declared next to the call hook in the engine's server module, so `contexts` and the other tool modules can emit events without depending on this module.

## Code Style

An event is a typed value; the envelope is added in one place, so no module writes raw JSON:

```rust
/// One thing that happened in a session.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AuditEvent {
    ToolCall { call_id: u64, tool: String, kind: ToolKind, arguments: Value },
    ToolResult { call_id: u64, ok: bool, duration_ms: u64, summary: String, result: Option<Value> },
    ContextSwitch { from: Context, to: Context, outcome: SwitchOutcome },
    // …
}
```

Follow the existing conventions: doc comments on every public item, no `unwrap` or `panic`, ports in `ports.rs`, adapters in `lib/adapters`.

## Testing Strategy

- **Engine unit tests:**
  - Every event serialises with the envelope and parses back; unknown fields are ignored by the reader.
  - `seq` continues across a resumed session; the sink resumes from the last line of an existing file.
  - The scrub removes every resolved secret value from nested JSON (strings inside arrays and objects) and from keys, leaves everything else, and uses exact matches only.
  - The caps: a result over 64 KB is cut with `truncated`, `size_bytes` and a `sha256` that matches the full result; `summary` at most 2 KB; no line over 160 KB.
  - Rendering matches the examples; a session with no `session_ended` is shown as ended at its last event; a truncated last line is ignored and reported.
  - A call whose `tool_call` cannot be recorded is refused and the tool is never run (with a failing fake sink).
- **Adapter tests** (`lib/adapters`): the file and directory permissions are `0600` and `0700`; the file is only ever opened for append; each event is on disk (synced) before the call returns; two sessions written at once stay separate; a session that spans a simulated restart continues in the same file with the right `seq`; a directory that cannot be written makes the sink fail.
- **Integration tests** (`services/cli/tests/`):
  - Run the server with a fake tool, connect a shim, call the tool, and check the file: `session_started`, `tool_call`, `tool_result`, in order, with the right context.
  - Restart the server with the shim connected and check `server_stopping`, `session_resumed` and a continuous `seq`.
  - A secret set in the environment, echoed back by a fake remote command, appears as `[redacted]` in the file and nowhere else.
  - `workbench mcp logs --sessions` and `--session` (and `--raw`) print what is described; `audit_sessions` and `audit_events` return the same events and do not copy them into their own result.
  - Make the directory unwritable mid-session: the next call is refused with `audit log unavailable`.
- Every row of the event table, every rule under "What is never written, and what is cut", "The session boundary" and "Durability and failure" has a test.

## Boundaries

- **Always:** write the `tool_call` line before the tool runs; append only; one JSON object per line with the envelope; scrub resolved secret values from every line; create the directory `0700` and the files `0600`; refuse a call that cannot be recorded; keep `seq` unbroken.
- **Ask first:** adding or renaming an event or a field that readers rely on; changing a size cap; adding a dependency (`serde_json`, a time library, `sha2`); a hash chain; copying audit data anywhere else.
- **Never:** rewrite, reorder or delete an event; write a secret; write an audit record in `developer-support`; copy a session into the storage backend; let a tool, a prompt or an agent disable or bypass the record; run a tool when its `tool_call` could not be written.

## Success Criteria

1. In `operational-support`, every tool call, context switch, context check, remote command, tunnel and error is a line in the session's file, and the `tool_call` line is on disk before the tool runs.
2. A session that spans a server restart is one file with an unbroken `seq`, containing `server_stopping` and `session_resumed`.
3. A session's start, end and resumption follow "The session boundary", including the 90-second window, and a session that was never closed is shown as ended at its last event.
4. No resolved secret value appears in any line, including values inside nested results and remote output.
5. Results are recorded up to 64 KB; a larger result is cut with its full size and digest; no line is longer than 160 KB.
6. If an event cannot be written, the call is refused with `audit log unavailable` and the tool does not run.
7. Files are `0600` in a `0700` directory, are only appended to, and are never rewritten or deleted by Workbench.
8. `workbench mcp logs --sessions` lists sessions; `--session ID` renders one; `--raw` prints the JSON lines as written.
9. `audit_sessions` and `audit_events` return sessions and events, page with `next_seq`, and do not copy the events into their own recorded result.
10. `developer-support` writes no session file and registers no audit tool.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench mcp logs --prune`: delete or archive sessions older than a date.
- A hash chain over the lines, so an edited file can be detected.
- A configurable result cap.
- Per-session summaries produced for people (for example a short list of what changed hands).

## Open Questions

1. **Fail closed:** I made a call that cannot be recorded not run at all, because operational access without a record is exactly what an audit is meant to prevent. The cost is that a full disk stops all operational tools until it is fixed. Is that right?
2. **Recording results:** I record each tool's structured result up to 64 KB, not only a summary, because `flow-memory` needs to see what an agent found. Results can hold production data. Is a 64 KB cap per result right, or do you want a different size or summaries only?
3. **Reading other sessions:** I let the retrieval tools read any session in the logs directory, not just the current one. Is that what you want, or should an agent only see its own session?
