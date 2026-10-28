# Spec: core

Module `core` from the [capability map](capability-map.md). Status: **approved.**

Depends on [`workspace`](SPEC-workspace.md) (approved). Used by [`cli`](SPEC-cli.md) for the server config template.

## Decisions

1. `workbench-mcp` is the MCP server: Rust, MCP over **stdio**, **one process per mode**.
2. `--mode <mode>` picks the mode. The default is `developer-support`. `operational-support` must be named explicitly.
3. **Each mode loads only what it needs.** `developer-support` never reads the server config and holds no secrets. `operational-support` requires a valid server config *before* the server starts; if it is missing or invalid the process exits with an error that says what to fix.
4. The server config lives at `.workbench/mcp/config.yml`. It lists the contexts and, for each, the path of that context's **own config file**. Secrets are written inline in a context file or pulled from an environment variable whose name the same file gives (`<field>_env`); the variable wins when it is set. Inline values stay allowed (confirmed). Every secret is a `secrecy` value from the moment it is read and is never printed, logged, serialised or returned by a tool.
5. `core` owns the *machinery*: argument parsing, the mode registry, the tool and prompt registry, the server config envelope and loader, the secret type and the start-up sequence. Each module owns the content of its own tools, prompts and config section.
6. `core` is a driving adapter around engine use cases. Tools contain no business rules.

## Assumptions (correct me now)

1. The modes `operational-onboarding` and `developer-onboarding` are accepted by `--mode` and listed in `--help` as "planned"; choosing one exits with `mode '<name>' is planned and not available yet`. Only `developer-support` and `operational-support` start.
2. In `stdio` there is one client per process, so the **session** is the process lifetime. `core` creates a session id at start; `SPEC-audit.md` can refine this.
3. The workspace is located like the CLI does it: search upward from the current directory for `.workbench/`, with `--workspace <DIR>` as an override.
4. *(Confirmed)* Storage settings (SQLite or Memgraph) live in the **workspace config**, not the server config, because `developer-support` and the CLI need storage without the server config. A Memgraph password is referenced by environment variable name, like the LLM key. `SPEC-workspace.md` is amended to match (see Open Questions).
5. There is no hot reload. Changing a config means restarting the server.
6. *(Confirmed)* The MCP SDK is the official Rust SDK (`rmcp`); its version is pinned when the dependency is added.

## Objective

Give every other module one predictable place to plug in, so a tool is "a function with a name, a schema and a kind" and the server decides when it exists, how it is called and what is kept out of the logs.

The module is done when:
- `workbench-mcp` starts in `developer-support` with no server config and lists its tools over stdio,
- `workbench-mcp --mode operational-support` refuses to start without a valid server config and says why, and starts with one,
- a secret can be loaded from a context file or from the environment variable that file names, and cannot be printed.

### Command line

```text
workbench-mcp [--mode <MODE>] [--workspace <DIR>]

--mode <MODE>       developer-support (default) | operational-support
                    | operational-onboarding (planned) | developer-onboarding (planned)
--workspace <DIR>   Workspace root. Default: search upward for .workbench/
```

### Modes and what they load

| Mode | Server config | Modules registered |
|---|---|---|
| `developer-support` (default) | Not read | `graph`, `semantic-layer`, `lsp` |
| `operational-support` | Required and valid | everything in `developer-support`, plus `contexts`, `audit`, `operations`, `data`, `execution-flow`, `flow-memory`, `incidents`, `agent-prompts` |

A mode is a list of modules. `core` registers each module's tools and prompts; a tool that is not in the mode's modules does not exist for the client.

### Start-up sequence

```text
parse arguments
  → locate the workspace            fail: "no .workbench/ found from <cwd>. Run 'workbench init'."
  → load and validate workspace config
  → if the mode needs the server config:
        load it and every context file it lists, resolve secret references, validate every section
        (collect all problems, report them together)
  → build adapters from the config (storage, then the mode's modules)
  → register tools and prompts for the mode
  → serve MCP on stdio
```

Any failure before "serve" exits non-zero, writes the reason to **stderr**, and writes nothing to stdout.

### Registry: tools and prompts

Each tool has: a snake_case name, a description, an input schema derived from a Rust struct, a **kind**, and a handler.

| Kind | Meaning | Examples |
|---|---|---|
| `observe` | Reads the code, the index or an observed system. Never changes an observed system. | `service_map`, `fetch_logs`, `db_query` |
| `record` | Writes only to Workbench's own storage or files. | `remember`, `save_incident_report`, `reindex` |

There is no third kind: **no tool changes an observed system**, in any mode. Every tool must declare its kind; the registry refuses to start with a tool that does not. `observe` tools are advertised to the client with the MCP read-only hint.

Conventions every module follows:
- Tools that query projects take an optional `projects` list (project ids); omitted means all projects (`SPEC-workspace.md`).
- Success returns structured JSON plus a short text summary. Failure is returned as a tool error with an actionable message, not a protocol error.
- No tool output contains a secret. Secrets are `Secret<T>` values and have no `Display` and no `Serialize`.
- Prompts are registered the same way, from `agent-prompts` only.

A **tool context** is passed to every handler: the workspace, the session id, the active context (operational modes) and the client's capabilities (for example whether it supports elicitation, which `contexts` needs). `core` also offers a call hook that runs before and after every tool call; `audit` uses it, `core` does not know about audit.

### Server config: the envelope

```yaml
# .workbench/mcp/config.yml   (git-ignored with the rest of .workbench/)
version: 1
default_context: staging        # required; owned by SPEC-contexts
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

**Context config files.** Each context has its own file. The server config gives its path: relative to the directory of the server config, or absolute. `core` loads every listed file at start-up; a missing, unreadable or invalid file is reported with its path together with the other problems. Because each file holds that context's credentials and settings, a person can keep, replace or share one without touching the others. All of them sit under `.workbench/`, which is git-ignored.

**Secret references.** For a secret field `X`, a context file may contain `X` (an inline value) and/or `X_env` (the *name* of an environment variable). Resolution, in order:
1. `X_env` is present and that variable is set: use its value (the environment wins).
2. Otherwise `X` is present inline: use it.
3. Otherwise it is an error that names the file, the field and the variable: `contexts/production.yml: postgres.password: set 'password' or set PROD_PG_PASSWORD`.

The workspace config's `api_key_env` and `password_env` fields use the same `_env` naming but allow no inline form.

**The template** written by `workbench init` is the server config plus one context file per default context (`production`, `staging`, `testing`), each stitched from the owning module's fragment. They are deliberately incomplete: placeholders such as `<fill in>` fail validation, so `operational-support` will not start until they are replaced.

## Tech Stack

- Rust 2024; `tokio` (already a workspace dependency, multi-thread runtime); `clap` for arguments; `secrecy` (already a workspace dependency); `serde` and `yaml_serde`; `thiserror`.
- MCP over stdio via the official Rust SDK, `rmcp` (new dependency, approval needed; see Boundaries).
- Logging to stderr. The logging crate, if one is added, is a new dependency and needs approval.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Run:      cargo run -p workbench-mcp -- --mode developer-support
          cargo run -p workbench-mcp -- --mode operational-support --workspace ./tmp
Inspect:  npx @modelcontextprotocol/inspector target/debug/workbench-mcp   # manual check
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

services/mcp/src/
├── main.rs        # wiring only
├── cli.rs         # clap: --mode, --workspace
├── startup.rs     # the start-up sequence above
├── serve.rs       # rmcp stdio server built from the Registry
└── modules/       # one file per module: its tools and prompts mapped onto engine use cases
```

The engine has no I/O: environment access sits behind the `EnvSource` port so tests can supply their own.

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
  - The registry rejects a tool without a kind and a duplicate tool name.
  - Secret references: a set `X_env` variable wins over inline `X`; inline is used when the variable is unset; neither is an error that names the file, field and variable; an unset `_env` variable with no inline value does not fall back silently.
  - Context files: relative paths resolve against the server config's directory; a missing or invalid file is reported with its path alongside other problems.
  - A config with several problems reports all of them at once.
  - `Secret`: `Debug` output contains no value; there is no `Display` or `Serialize` (a compile-time check with `static_assertions`-style trait bounds, or a documented compile-fail case).
- **Adapter tests** (`lib/adapters`): the YAML loader on valid, invalid and placeholder files; the template round-trips and fails validation until placeholders are replaced.
- **Integration tests** (`services/mcp/tests/`): spawn the real binary and speak JSON-RPC over stdio with `std::process::Command`:
  - `developer-support` starts with no server config, even when a *garbage* `.workbench/mcp/config.yml` is present, because it never reads it; `tools/list` returns exactly its tools.
  - `operational-support` without a server config, or with placeholders, exits non-zero with a message naming the file, writes nothing to stdout.
  - stdout contains only protocol messages; logs and errors go to stderr.
  - A known secret value set through the environment never appears in stdout or stderr, even at the most verbose log level.
- Each row of the mode table and each rule in "Registry" has a test.

## Boundaries

- **Always:** log to stderr only; fail fast, before serving; report every configuration problem together; derive tool schemas from the Rust types.
- **Ask first:**
  - Adding a dependency: `rmcp`, a logging crate, `static_assertions`.
  - Adding a mode, or changing what an existing mode registers.
  - Changing the server config `version`, or the secret-reference convention (`X` / `X_env`).
- **Never:** read the server config in `developer-support`; write anything to stdout except MCP messages; print, log, serialise or return a secret; register a tool that can change an observed system.

## Success Criteria

1. `workbench-mcp` with no arguments starts `developer-support` with no server config present, and `tools/list` over stdio returns its tools.
2. A garbage server config file does not affect `developer-support`.
3. `--mode operational-support` without a valid server config exits non-zero, names the file and every problem, and prints nothing to stdout. A valid config starts it.
4. `--mode operational-onboarding` and `--mode developer-onboarding` exit with the "planned" message.
5. A secret written inline and also named by `X_env` resolves to the environment value; with the variable unset it resolves to the inline value; with neither it is an error naming the file, field and variable. No value appears in any output.
6. Every registered tool has a kind; the registry refuses one without.
7. Each context's config is loaded from the path in the server config (relative to the server config's directory, or absolute); a missing or invalid context file is reported with its path.
8. The templates written by `workbench init` (server config and one file per context) fail validation until their placeholders are replaced.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

None. The questions from the first draft are answered and folded into the sections above.
