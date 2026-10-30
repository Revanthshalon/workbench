# Spec: core

Module `core` from the [capability map](capability-map.md). Status: **approved.**

Depends on [`workspace`](SPEC-workspace.md) (approved). Used by [`cli`](SPEC-cli.md), which hosts the `workbench mcp` commands and the server config template.

## Decisions

1. **One binary.** There is no separate `workbench-mcp` executable and no `services/mcp` crate. The MCP server is the `mcp` command group of `workbench`; its code is the `mcp` module of the `workbench` crate (`services/cli/src/mcp/`). The engine keeps its "no I/O" rule, so the sockets, process handling, logs and the MCP transport live in that module, not in `lib/engine`.
2. **Run model.** A **server** runs in the background, one per workspace and mode, and listens on a **Unix domain socket**. AI platforms are configured to run `workbench mcp connect`, a **stdio shim** that relays between the platform and the server. The CLI manages the server's life: `start`, `stop`, `restart`, `status`, `logs`.
3. `--mode <mode>` picks the mode. The default is `developer-support`. `operational-support` must be named explicitly.
4. **Each mode loads only what it needs.** `developer-support` never reads the server config and holds no secrets. `operational-support` requires a valid server config *before* the server starts; if it is missing or invalid, `start` fails with an error that says what to fix, and no server is left running.
5. The server config lives at `.workbench/mcp/config.yml`. It lists the contexts and, for each, the path of that context's **own config file**. Secrets are written inline in a context file or pulled from an environment variable whose name the same file gives (`<field>_env`); the variable wins when it is set. Inline values stay allowed (confirmed). Every secret is a `secrecy` value from the moment it is read and is never printed, logged, serialised or returned by a tool.
6. `core` owns the *machinery*: the mode registry, the tool and prompt registry, the server config envelope and loader, the secret type, the start-up sequence, and the server life cycle (socket, shim, logs). Each module owns the content of its own tools, prompts and config section.
7. `core` is a driving adapter around engine use cases. Tools contain no business rules.
8. **Connections are checked before the server starts, for operational modes only, and only for the context in use.** `operational-support` does not start unless the connections of its start context pass the context check (`workbench context check`, `SPEC-cli.md`, details in `SPEC-contexts.md`). There is no health monitoring while the server runs and no "degraded" state. `developer-support` makes no connection checks at all.

## Assumptions (correct me now)

1. The modes `operational-onboarding` and `developer-onboarding` are accepted by `--mode` and listed in `--help` as "planned"; choosing one exits with `mode '<name>' is planned and not available yet`. Only `developer-support` and `operational-support` start.
2. A **session** is one connected client, identified by a token the shim generates when it starts. The token survives a server restart, so a session (and its audit trail) is not split by `restart`. `SPEC-audit.md` defines retention.
3. The workspace is found as the CLI does it: search upward from the current directory for `.workbench/`, with `--workspace <DIR>` as an override.
4. *(Confirmed)* Storage settings (Memgraph's host, port and namespace) live in the **workspace config**, not the server config, because `developer-support` and the CLI need storage without the server config.
5. There is no hot reload. Changing a config means `workbench mcp restart`.
6. *(Confirmed)* The MCP SDK is the official Rust SDK (`rmcp`); its version is pinned when the dependency is added. It is used with a custom transport over the socket (to be confirmed when the dependency is added).
7. Unix-like systems (macOS, Linux) first. A Windows transport (named pipes) is not in this spec.
8. *(Confirmed)* The per-session log file **is** the audit record. There is no separate copy in the storage backend. It is a JSON Lines file (one event per line) so that other modules can read it; `workbench mcp logs --session` renders it for people (`--raw` shows the lines as written). `SPEC-audit.md` defines the event format.

## Objective

Give every other module one predictable place to plug in, so a tool is "a function with a name, a schema and a kind", and let a person run the server in the background and keep using the CLI to see its logs, check on it and restart it.

The module is done when:
- `workbench mcp start` brings up `developer-support` with no server config, and a platform that runs `workbench mcp connect` can list its tools,
- `workbench mcp start --mode operational-support` refuses to start without a valid server config and says why, and starts with one,
- `workbench mcp restart` replaces the server while a connected platform keeps working (it does not have to be restarted),
- `workbench mcp logs -f` follows the server's log,
- a secret can be loaded from a context file or from the environment variable that file names, and cannot be printed.

### The `workbench mcp` commands

All take `--workspace <DIR>` (default: search upward for `.workbench/`) and, where a mode applies, `--mode <MODE>`:
`developer-support` (default) | `operational-support` | `operational-onboarding` (planned) | `developer-onboarding` (planned).

| Command | Behaviour |
|---|---|
| `workbench mcp start [--mode M] [--context C]` | Starts the server in the background and waits until it reports ready or failed. For `operational-support` it first runs the context check for `C` (default: `default_context`) and refuses to start if any configured connection fails; `C` is the context every new session starts in. `--context` is rejected for other modes. Prints the pid, the socket path and the log path. On failure prints the reasons (the same ones `serve` would print), exits non-zero and leaves nothing running. Fails if a server for that workspace and mode is already running. |
| `workbench mcp serve [--mode M]` | Runs the server in the foreground and logs to stderr. What `start` runs in the background; also useful for debugging. |
| `workbench mcp stop [--mode M \| --all]` | Asks the server to shut down cleanly and waits. Connected shims keep waiting for it (see Shim). |
| `workbench mcp restart [--mode M]` | `stop` then `start`. Reloads the configs. Connected shims reconnect on their own. |
| `workbench mcp status [--mode M]` | For each running server: mode, pid, uptime, socket, number of connected clients, log path. Removes stale pid files it finds. |
| `workbench mcp logs [--mode M] [--session ID \| --sessions] [-f] [-n N]` | By default prints the last `N` lines (default 50) of the mode's latest server log; `-f` keeps following. `--sessions` lists the session logs (session id, start time, context); `--session ID` prints that session's log, rendered for reading (`--raw` prints the JSON Lines as written). Works after the server has stopped. |
| `workbench mcp connect [--mode M] [--start]` | The stdio shim for AI platforms: relays between stdin/stdout and the server. Never writes anything to stdout except MCP messages. With no server running it exits with `no <mode> server running; run 'workbench mcp start --mode <mode>'`. `--start` starts one first, and is allowed for `developer-support` only. |

### Modes and what they load

| Mode | Server config | Modules registered |
|---|---|---|
| `developer-support` (default) | Not read | `graph`, `semantic-layer`, `lsp`, `dev-support` |
| `operational-support` | Required and valid | everything in `developer-support`, plus `contexts`, `audit`, `operations`, `data`, `execution-flow`, `flow-memory`, `incidents`, `agent-prompts` |

A mode is a list of modules. `core` registers each module's tools and prompts; a tool that is not in the mode's modules does not exist for the client. Several servers (one per mode) can run at once in the same workspace; each has its own socket and log.

### Start-up sequence (inside the server)

```text
locate the workspace            fail: "no .workbench/ found from <cwd>. Run 'workbench init'."
  → load and validate workspace config
  → if the mode needs the server config:
        load it and every context file it lists and validate every section; resolve secret references for the start context only
        (collect all problems, report them together)
  → if the mode connects to systems (operational-support):
        run the context check for the start context; any failed connection stops the start
  → build adapters from the config (storage, then the mode's modules)
  → register tools and prompts for the mode
  → bind the socket, write the pid file, report ready
  → accept connections
```

A failed context check is reported like any other start-up failure, naming each system that failed and why (never with a secret). Any failure before "report ready" is written to the log and reported to `start` (or to stderr with `serve`); no pid file or socket is left behind.

### Run files

| Path | Content |
|---|---|
| `.workbench/run/mcp-<mode>.sock` | The server's Unix socket. Directory mode `0700`, socket mode `0600`: only the same user can connect. |
| `.workbench/run/mcp-<mode>.pid` | The server's pid. A pid file whose process is gone is stale and is removed by `start` and `status`. |
| `<logs.dir>/server-<mode>-<YYYYMMDD-HHMMSS>.log` | The server's own log, one file per server start: start-up, context check results, connections, restarts, errors. Timestamped lines with a level. Never contains a secret. |
| `<logs.dir>/sessions/<session-id>.log` | `operational-support` only: one file per session, the audit record of what happened in it (see Logs). |

There is no TCP or HTTP listener. Nothing is reachable from the network.

### Logs

| Log | Written when | Contains |
|---|---|---|
| **Server log** (`server-<mode>-<timestamp>.log`) | Always. A new file for every server start, so no rotation is needed. | Start-up steps, the context check result, socket and connection events, restarts, shutdown, errors. |
| **Session log = the audit record** (`sessions/<session-id>.log`) | `operational-support` only. One file per session. | A record of the session, as JSON Lines: when it began and ended, the context it started in, each tool call (name, arguments, duration, outcome, a short summary and the result, capped at 64 KB), context switches (with the check result), and errors, including connections that went down. |

- The directory is `logs.dir` in the server config, relative to the workspace root or absolute; the default is `.workbench/logs`. `developer-support` never reads the server config, so it always uses the default directory and writes only server logs.
- The session log is the **only** audit record: nothing is copied to the storage backend. The server only appends to it and never rewrites or deletes events. It is JSON Lines so `flow-memory` and other readers can parse it; `workbench mcp logs --session ID` renders it for people and `--raw` prints the lines as written. The event format is defined in `SPEC-audit.md`.
- A session is identified by the shim's token, so a session that continues across a `restart` keeps appending to the same file. On start the server also closes, in the audit record, any session left open by an earlier server (`SPEC-audit.md`, "The session boundary"). An event that cannot be written makes the call it belongs to fail, so nothing runs unrecorded.
- `workbench mcp logs` finds the directory the same way. For `operational-support` it reads `logs.dir` from the server config; if the config cannot be read it says so and falls back to the default directory.
- Logs can hold query results and log lines from production systems, so the directory is created `0700` and every file `0600`. No secret ever appears in a log: secrets are never exposed outside the point where a connection is opened.
- There is no rotation and no automatic cleanup in the first release; see the Roadmap.

### The socket and the shim

After a client connects, the first message tells the server what it is:
- `client` with a session token: then the connection carries MCP, framed exactly as on stdio (one JSON-RPC message per line).
- `control`: then the connection answers `status` and `stop` requests for the CLI and carries no MCP.

The shim:
- generates its session token once, and sends it each time it connects;
- relays bytes between its stdin/stdout and the socket;
- remembers the platform's `initialize` request and `initialized` notification. If the server goes away and comes back (a `restart`), the shim reconnects (retrying for up to 90 seconds), **replays that handshake** to the new server, swallows the new server's reply, and carries on. The platform never sees the restart, except that requests that were in flight get a JSON-RPC error `server restarting, retry`;
- exits with a clear message on stderr, and a non-zero code, if the server does not come back in time.

### Sessions and state

State that belongs to a client is per session, not per server: the active context (`SPEC-contexts.md`) and the elicitation prompt for switching, which goes to the client that asked. Several platforms can use one server at the same time without affecting each other.

A new session starts in the server's start context (`--context`, default `default_context`); after a `restart` a session starts there again. Switching a session to another context (only on the user's explicit instruction, confirmed through elicitation) first runs the context check for the target and refuses the switch if it fails, so a context's connections are only checked when it is about to be used.

A **tool context** is passed to every handler: the workspace, the session id, the session's active context (operational modes) and the client's capabilities (for example whether it supports elicitation, which `contexts` needs). `core` also offers a call hook that runs before and after every tool call; `audit` uses it, `core` does not know about audit.

### Registry: tools and prompts

Each tool has: a snake_case name, a description, an input schema derived from a Rust struct, a **kind**, and a handler.

| Kind | Meaning | Examples |
|---|---|---|
| `observe` | Reads the code, the index or an observed system. Never changes an observed system. | `service_map`, `fetch_logs`, `db_query` |
| `record` | Writes only to Workbench's own storage or files. | `remember`, `save_incident_report`, `reindex` |
| `edit` | Changes files inside a registered project's directory. | `apply_patch`, `create_file`, `rename_symbol`, `apply_code_action` |

There are three kinds. **No tool changes an observed system** (a running environment or its datastores), in any mode. Only `developer-support` registers `edit` tools; `operational-support` registers none, and the registry refuses an `edit` tool for any other mode. `edit` tools follow these rules: they write only inside a registered project's directory (paths are resolved first, so a symlink cannot escape it); they refuse `.git/` and `.workbench/`; a patch applies only if the file is unchanged since the agent read it (compared by content hash), otherwise it fails and returns the current hash; writes are atomic; they return the diff they applied; and they index the files they touched right away. They are advertised to the client as not read-only, and they ask for no confirmation of their own: the AI platform's permission prompts apply. `developer-support` writes no session log, so git is the record of edits. Every tool must declare its kind; the registry refuses to start with a tool that does not. `observe` tools are advertised to the client with the MCP read-only hint.

Conventions every module follows:
- Tools that query projects take an optional `projects` list (project ids); omitted means all projects (`SPEC-workspace.md`).
- Success returns structured JSON plus a short text summary. Failure is returned as a tool error with an actionable message, not a protocol error.
- No tool output contains a secret. Secrets are `Secret<T>` values and have no `Display` and no `Serialize`.
- Prompts are registered the same way: the developer prompts by `dev-support` (`SPEC-dev-support.md`), the operational prompts by `agent-prompts` (`SPEC-agent-prompts.md`), and `enrich_knowledge` once by `semantic-layer`, which both modes load.

### Server config: the envelope

```yaml
# .workbench/mcp/config.yml   (git-ignored with the rest of .workbench/)
version: 1
default_context: staging        # required; owned by SPEC-contexts
logs:
  dir: .workbench/logs          # optional; where server and session logs are written
contexts:                       # name -> path of that context's own config file
  production: contexts/production.yml
  staging:    contexts/staging.yml
  testing:    contexts/testing.yml
```

```yaml
# .workbench/mcp/contexts/production.yml   (the content is owned by SPEC-contexts; illustrative)
version: 1
branch: main
postgres:
  host: db.prod.internal
  user: readonly
  password_env: PROD_PG_PASSWORD     # the variable's name; or write `password: ...` inline
```

`core` defines the envelope, loading and validation machinery. The content of a context file is owned by the modules that use it (`contexts`, and the tools behind it) and plugged in through one trait.

| `core` owns | A section's owner provides |
|---|---|
| File location, YAML parsing, `version` check | The section's types |
| The `Secret` type and secret-reference resolution | Which fields are secrets |
| Collecting all validation problems into one report | Validation of its section |
| Stitching the template for `workbench init` | A template fragment with placeholders |

**Context config files.** Each context has its own file. The server config gives its path: relative to the directory of the server config, or absolute. `core` loads and validates every listed file at start-up, but resolves a context's secret references only for the start context at start and for another context when a session switches to it (`SPEC-contexts.md`), so the secrets of an environment nobody is using stay out of memory; a missing, unreadable or invalid file is reported with its path together with the other problems. Because each file holds that context's credentials and settings, a person can keep, replace or share one without touching the others. All of them sit under `.workbench/`, which is git-ignored.

**Secret references.** For a secret field `X`, a context file may contain `X` (an inline value) and/or `X_env` (the *name* of an environment variable). Resolution, in order:
1. `X_env` is present and that variable is set: use its value (the environment wins).
2. Otherwise `X` is present inline: use it.
3. Otherwise it is an error that names the file, the field and the variable: `contexts/production.yml: postgres.password: set 'password' or set PROD_PG_PASSWORD`.

The workspace config's `api_key_env` and `password_env` fields use the same `_env` naming but allow no inline form.

**The template** written by `workbench init` is the server config plus one context file per default context (`production`, `staging`, `testing`), each stitched from the owning module's fragment. They are deliberately incomplete: placeholders such as `<fill in>` fail validation, so `operational-support` will not start until they are replaced.

## Tech Stack

- Rust 2024; `tokio` (already a workspace dependency, multi-thread runtime; the `net`, `io-util`, `signal`, `process`, `fs` and `time` features are added, see Boundaries); `clap` for arguments; `secrecy` (already a workspace dependency); `serde` and `yaml_serde`; `thiserror`.
- MCP via the official Rust SDK, `rmcp`, over a custom transport on the Unix socket (new dependency, approval needed; see Boundaries).
- Logging to a file for the background server and to stderr for `serve`. A logging crate, if one is added, is a new dependency and needs approval.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Run:      cargo run -p workbench -- mcp start
          cargo run -p workbench -- mcp start --mode operational-support --workspace ./tmp
          cargo run -p workbench -- mcp logs -f
          cargo run -p workbench -- mcp restart
Inspect:  npx @modelcontextprotocol/inspector target/debug/workbench mcp connect   # manual check
```

## Project Structure

```
lib/engine/src/server/
├── mode.rs        # Mode, ModeSpec: which modules a mode registers, whether it needs the server config
├── registry.rs    # ToolSpec, ToolKind, PromptSpec, Registry (refuses tools without a kind)
├── session.rs     # SessionId, ToolContext, call hook trait
├── secret.rs      # Secret<T> over secrecy: redacted Debug, no Display, no Serialize
├── config.rs      # ServerConfig envelope, ConfigSection trait, ConfigReport (all problems)
└── env.rs         # secret-reference resolution (`X` inline or `X_env`) over an EnvSource port, no std::env here

lib/engine/src/ports.rs      # + ServerConfigSource, EnvSource
lib/adapters/src/            # + server_config_file.rs (YAML + template), env_process.rs (std::env)

services/cli/src/mcp/         # the MCP server: a module of the `workbench` crate (no separate crate)
├── mod.rs         # entry points called by the `mcp` command group: start, serve, stop, restart, status, logs, connect
├── startup.rs     # the start-up sequence above
├── daemon.rs      # socket listener, one session per client connection, clean shutdown
├── shim.rs        # stdio <-> socket relay with handshake replay and reconnect
├── lifecycle.rs   # run files, readiness, stale pid cleanup, detaching the background process
├── control.rs     # the control connection: status and stop
├── logs.rs        # server and per-session log files, tail and follow, session listing
└── modules/       # one file per module: its tools and prompts mapped onto engine use cases

services/cli/src/cli.rs      # + the `mcp` command group (SPEC-cli)
```

The engine has no I/O: environment access sits behind the `EnvSource` port so tests can supply their own. Process, socket and log-file handling live in `services/cli/src/mcp/`, the only place that needs them.

## Code Style

A secret cannot be accidentally printed or serialised:

```rust
/// A secret value. It can only be read by calling [`Secret::expose`].
pub struct Secret(SecretString);

impl Secret {
    /// Returns the secret. Call this only where a connection is opened.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
```

A tool is registered with its kind or not at all:

```rust
registry.tool(ToolSpec::observe("fetch_logs", "Fetch logs from the active context", schema))?;
```

Follow the existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in `ports.rs`, adapters in `lib/adapters`.

## Testing Strategy

- **Engine unit tests:**
  - The mode table: each mode lists the modules above and nothing else.
  - The registry rejects a tool without a kind and a duplicate tool name, and an `edit` tool in any mode but `developer-support`.
  - `edit` rules: a path outside the project, a symlink out of it, and `.git/` or `.workbench/` are refused; a stale content hash is refused with the current hash; a successful edit returns a diff and triggers an incremental index of the touched files.
  - Secret references: a set `X_env` variable wins over inline `X`; inline is used when the variable is unset; neither is an error that names the file, field and variable; an unset `_env` variable with no inline value does not fall back silently.
  - Context files: relative paths resolve against the server config's directory; a missing or invalid file is reported with its path alongside other problems.
  - A config with several problems reports all of them at once.
  - `Secret`: `Debug` output contains no value; there is no `Display` or `Serialize` (a compile-time check with `static_assertions`-style trait bounds, or a documented compile-fail case).
  - Per-session state: two sessions have independent active contexts, and each starts in the server's start context.
  - Context check gate (with fake connection probes): `operational-support` is refused when any probe fails and names it; unconfigured systems are skipped; `developer-support` runs no probes; a context switch runs the target's probes first and is refused on failure; no probe runs after start-up.
- **Adapter tests** (`lib/adapters`): the YAML loader on valid, invalid and placeholder files; the template round-trips and fails validation until placeholders are replaced.
- **Integration tests** (`services/cli/tests/`, running the real `workbench` binary in a temp workspace; JSON-RPC written by hand over `std::process::Command`, no extra dependency):
  - Life cycle: `start` then `status` shows the server; `start` again fails; `stop` removes the socket and pid file; a stale pid file is cleaned up; `start` for `operational-support` without a valid config fails, names the file and leaves no pid file or socket.
  - `developer-support` starts with no server config, even with a *garbage* `.workbench/mcp/config.yml` present, because it never reads it; `tools/list` through `connect` returns exactly its tools.
  - Restart: with a shim connected, `restart` happens, the shim reconnects and replays the handshake, and a later `tools/list` succeeds without the test client re-initialising; a request in flight during the restart gets the `server restarting` error.
  - Two shims at once keep separate sessions.
  - `connect` with no server running exits non-zero with the `start` hint; its stdout stays empty.
  - The socket directory is `0700` and the socket `0600`.
  - Session log as audit record: every tool call, context switch and error in a session appears as one JSON line in that session's file; the lines parse; events are only ever appended; nothing about the session is written to the storage backend.
  - Logs: `logs` prints the last lines of the latest server log and `-f` follows new ones; every `start` creates a new server log; in `operational-support` each session gets its own file under `logs.dir` (read from the server config), and a session that spans a `restart` keeps appending to the same file; `developer-support` writes no session logs and uses the default directory; `--sessions` and `--session` list and print them; the logs directory is `0700` and the files `0600`; a known secret value never appears in any log, stdout or stderr, even at the most verbose log level.
- Each row of the mode table, each command and each rule in "Registry" has a test.

## Boundaries

- **Always:** keep stdout of `connect` for MCP messages only; log to the files described under Logs (or stderr for `serve`); create the logs directory `0700` and every log file `0600`; fail fast before reporting ready; report every configuration problem together; derive tool schemas from the Rust types; create run files with the permissions above.
- **Ask first:**
  - Adding a dependency or tokio feature: `rmcp`, a logging crate, `static_assertions`, the extra tokio features.
  - Adding a mode, or changing what an existing mode registers.
  - Changing the server config `version`, or the secret-reference convention (`X` / `X_env`).
  - Any network (TCP or HTTP) transport, or a Windows transport.
- **Never:** register an `edit` tool outside `developer-support`; let an `edit` tool write outside a registered project's directory; listen on a network interface; read the server config in `developer-support`; write anything to stdout from `connect` except MCP messages; print, log, serialise or return a secret; register a tool that can change an observed system; start a server on `connect` unless `--start` is given for `developer-support`.

## Success Criteria

1. `workbench mcp start` brings up `developer-support` with no server config present; a platform running `workbench mcp connect` can list its tools.
2. A garbage server config file does not affect `developer-support`.
3. `start --mode operational-support` without a valid server config exits non-zero, names the file and every problem, and leaves no server, pid file or socket. A valid config starts it.
4. `--mode operational-onboarding` and `--mode developer-onboarding` exit with the "planned" message.
5. `status` shows running servers with their client counts and removes stale pid files; `stop` and `restart` do what the table says; `logs -f` follows the log, `logs --sessions` lists session logs and `logs --session ID` prints one.
    - In `operational-support` each session has its own log file in the directory given by `logs.dir` in the server config; a session that spans a restart keeps one file. Every server start has its own server log. Log files are owner-only and never contain a secret.
6. After `restart`, a connected shim reconnects and the platform keeps working without being restarted; a request in flight gets a `server restarting` error instead of hanging.
7. Two clients connected at once have separate sessions and separate active contexts.
8. The socket is reachable only by the owner (`0600` in a `0700` directory), and no network port is opened.
9. A secret written inline and also named by `X_env` resolves to the environment value; with the variable unset it resolves to the inline value; with neither it is an error naming the file, field and variable. No value appears in any output or log.
10. Every registered tool has a kind (`observe`, `record` or `edit`); the registry refuses one without.
    - `edit` tools exist only in `developer-support`, write only inside registered project directories (a symlink escape and `.git/` or `.workbench/` are refused), apply a patch only to a file unchanged since it was read, return the diff, and index what they touched.
    - `start --mode operational-support` runs the context check for its start context and refuses to start, naming each failing system, if any connection fails; nothing is left running. With all connections ok it starts.
    - `developer-support` runs no connection checks.
    - Switching a session to another context runs that context's check first and refuses the switch if it fails.
    - While the server runs, a tool whose connection has gone down returns an error naming the system; there is no background monitoring.
11. Each context's config is loaded from the path in the server config (relative to the server config's directory, or absolute); a missing or invalid context file is reported with its path.
12. The templates written by `workbench init` (server config and one file per context) fail validation until their placeholders are replaced.
13. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- Streamable HTTP as an opt-in listener (`workbench mcp start --http`), localhost only, with a bearer token and `Origin` checking. Until then there is no network listener.
- A Windows transport (named pipes).
- Log retention: deleting or archiving old server and session logs (for example `workbench mcp logs --prune`). Until then logs are kept until a person removes them.

## Open Questions

None. The questions from the first draft are answered and folded into the sections above.
