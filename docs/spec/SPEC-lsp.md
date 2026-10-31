# Spec: lsp

Module `lsp` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`workspace`](SPEC-workspace.md) (approved) and [`core`](SPEC-core.md) (approved). Used by `graph` and `execution-flow` (symbol and reference lookups), by `dev-support`, and by `platform-setup`, which reuses the server catalog and the `workbench lsp run` launcher.

## Decisions already made elsewhere

1. **Two entry points on one shared launcher.** `workbench lsp run <project-id>` runs a project's language server over stdio, so an AI platform can use it as its own LSP server. The MCP server starts the same servers lazily and exposes them as tools.
2. **One server per project (and view).** Language servers are activated per project and used at workspace level: a request about a file is routed to the server of the project that owns the file.
3. **Settings come from the workspace config** (`SPEC-workspace.md`): per project `lsp` (`enabled`, `server`, `command`, `settings`), per-language defaults at the top level, with the precedence project over workspace default over catalog default, merged field by field. `environment.venv` and `environment.activate` say how to run tools in the project's environment.
4. `basedpyright` is the default for Python. The servers for the other languages are chosen here.
5. **Views.** In `operational-support` a session's LSP tools see the active context's branch, so servers run against that view's worktree with the project's own environment (`SPEC-contexts.md`). `developer-support` uses the project's own working tree. How a worktree gets the dependencies a server needs is decided here.
6. LSP-backed tools are `lsp_diagnostics`, `lsp_definition`, `lsp_references`, `lsp_hover` and `lsp_symbols` (kind `observe`), and `rename_symbol` and `apply_code_action` (kind `edit`, `developer-support` only, `SPEC-core.md`). `operational-support` registers only the `observe` tools.
7. Language servers need no secrets.

## Assumptions (correct me now)

1. **A project needs several servers, one per kind of file.** A Python project also has YAML, a Dockerfile and a Compose file; an Angular project has TypeScript, HTML, CSS and SCSS besides those. The server for a file is picked by the file's **kind**: `python`, `typescript`, `html`, `css`, `scss`, `yaml`, `dockerfile` or `compose`. The workspace config needs a small amendment to say this (Open Question 1).
2. **Workbench does not install language servers.** A person installs them (for example `uv tool install basedpyright` or `npm install -g typescript-language-server typescript`). A missing server fails with the catalog's install hint, and `workbench lsp check` reports it.
3. **`workbench lsp run` is a pass-through launcher.** It starts the server with the right working directory and environment and then hands over stdin and stdout, without sitting in the middle of the protocol. Workbench does not rewrite the protocol, so `lsp.settings` cannot be injected into that route (Open Question 2). They are applied on the MCP side, where Workbench is the client.
4. **The MCP side opens files on demand.** For a query, Workbench opens the file with its current contents from disk and closes it afterwards, so answers always reflect what is on disk; it keeps no editor buffers.
5. Positions in tool inputs and outputs are **1-based line and 1-based column in characters**. Workbench converts to and from LSP's 0-based UTF-16 offsets by reading the line from disk.
6. Servers started by the MCP server are shared by every session in that server process, because a server belongs to a project and view, not to a session.

## Objective

Give people and agents the answers a language server gives (where is this defined, who uses it, what are the errors here, what does this type say), for every project in the workspace, using each project's own Python or Node environment, without anyone configuring a server by hand for each project.

The module is done when:
- every file in a project is routed to the right language server for its kind,
- `workbench lsp run <project-id>` starts that server with the project's environment and works as an AI platform's LSP server,
- the MCP tools answer from the right project's server, on the active context's branch in operational mode,
- a missing or crashed server gives a clear message, not a hang.

### The catalog

The catalog says, for each file kind, which server is the default and how to start it. A `lsp.<…>.server` or `command` in the workspace config overrides it.

| Kind | Files | Default server | Command | Install hint |
|---|---|---|---|---|
| `python` | `*.py`, `*.pyi` | `basedpyright` | `basedpyright-langserver --stdio` | `uv tool install basedpyright` |
| `typescript` | `*.ts`, `*.tsx` | `typescript-language-server` | `typescript-language-server --stdio` | `npm install -g typescript typescript-language-server` |
| `html` | `*.html` | `ngserver` in `angular` projects, else `vscode-html-language-server` | `ngserver --stdio --tsProbeLocations … --ngProbeLocations …`, or `vscode-html-language-server --stdio` | `npm install -g @angular/language-server`, or `vscode-langservers-extracted` |
| `css`, `scss` | `*.css`, `*.scss` | `vscode-css-language-server` | `vscode-css-language-server --stdio` | `npm install -g vscode-langservers-extracted` |
| `yaml` | `*.yml`, `*.yaml` that are not Compose files | `yaml-language-server` | `yaml-language-server --stdio` | `npm install -g yaml-language-server` |
| `dockerfile` | `Dockerfile`, `Dockerfile.*`, `*.dockerfile` | `docker-langserver` | `docker-langserver --stdio` | `npm install -g dockerfile-language-server-nodejs` |
| `compose` | the project's Compose file, `compose.y*ml`, `docker-compose*.y*ml` | `docker-compose-langserver` | `docker-compose-langserver --stdio` | `npm install -g @microsoft/compose-language-service` |

The program names and flags are checked when the catalog is built; the table is the starting point.

### Routing

1. A file path is matched to the project whose path contains it (the longest matching path wins). Outside every project: an error naming the path and the projects.
2. The file's **kind** comes from its name and extension (table above). A file of no listed kind: an error saying there is no language server for it.
3. The **effective settings** for that project and kind are the project's `lsp` for that kind, over the workspace default for the project's language, over the catalog (`SPEC-workspace.md` precedence).
4. `enabled: false` turns that server off for that project and kind: the tools say it is disabled.

### Starting a server

The same rules for `lsp run` and for the MCP side.

- **Working directory:** the project's path, or in operational mode the view's worktree (`.workbench/worktrees/<context>/<project-id>/`).
- **Command:** the effective `command`, else the catalog's. It is an argument list, never a shell string.
- **Environment:** if `environment.activate` is set it is the command prefix (for example `uv run` or `poetry run`). Otherwise, if `environment.venv` is set, the child gets `VIRTUAL_ENV` set to it and its `bin/` first on `PATH`. The child's environment is a short allow-list (`PATH`, `HOME`, the locale and terminal variables, and the variables above), and **never contains a variable that any Workbench config names as a secret source** (a `*_env` field), so production credentials cannot leak into a language server or its plugins.
- **Dependencies in a worktree.** A worktree holds only the files git tracks. For Python, the project's virtual environment is reused as it is (the server resolves the project's own modules from the worktree first, then third-party packages from the environment; to be confirmed with `basedpyright`). For Node projects, the project's `node_modules` is linked into the worktree when the lock file in the worktree is identical to the project's; if it differs, the server still starts and the first answer carries a warning that dependencies may not match that branch (Open Question 3). Workbench never installs packages and never runs a project's scripts.
- **Missing program:** fails with `language server '<name>' not found. Install it: <hint>`.

### `workbench lsp` (commands)

| Command | Behaviour |
|---|---|
| `workbench lsp run <project-id> [--kind <KIND>] [--view <working\|context:NAME>]` | Starts the server for that project and kind (default kind: the project's main one, `python` or `typescript`) and hands over stdin and stdout, so an AI platform can use it as its LSP server. `--view` defaults to `working`. Exits with the server's exit code. |
| `workbench lsp list` | For every project, each kind with its server, command, activation, and whether the program was found. Read-only. |
| `workbench lsp check [<project-id>]` | Starts each enabled server for the project (or all projects), runs the LSP handshake and shuts it down, and reports `ok`, `failed` or `not found` with the time taken. Read-only. |

All find the workspace like the other commands. `lsp run` writes nothing to stdout except what the server writes; its own messages go to stderr.

### The MCP side

- **Server pool.** The MCP server keeps one running server per project, view and kind. They start lazily on the first request, are shared by sessions, shut down cleanly after 10 idle minutes, and are evicted least-recently-used beyond 8 running at once. A server that crashes is restarted once on the next request, with a short back-off; a second crash in a row is reported.
- **Handshake.** Workbench acts as the LSP client: it sends `initialize` with the project (or worktree) as the root, the effective `settings` (answering `workspace/configuration`), then `initialized`. Startup has a time limit (60 seconds by default) and every request a time limit (30 seconds by default); on a timeout the tool says which server and request.
- **Documents.** A file is opened with its current contents from disk for the duration of a query (`didOpen`, `didClose`), so results reflect the disk. After an `edit` tool changes files, the touched files are re-opened on the next query.
- **Results.** Paths are relative to the project root together with the project id; positions are 1-based. Large results are cut: references and symbols to 200 by default (`limit`, at most 2 000), hover text to 8 KB, and the result says it was cut.

### Tools (candidates; the registry fixes the final names)

| Tool | Kind | Input | Result |
|---|---|---|---|
| `lsp_diagnostics` | observe | `path`, or a `project` for all its open files | errors, warnings and hints with range, severity, source and message |
| `lsp_definition` | observe | `path`, `line`, `column` | the definition locations |
| `lsp_references` | observe | `path`, `line`, `column`, `limit` | the reference locations in that project |
| `lsp_hover` | observe | `path`, `line`, `column` | type and documentation text |
| `lsp_symbols` | observe | `path` (symbols in a file) or `query` with `projects` (workspace symbols) | symbol names, kinds and locations |
| `rename_symbol` | edit | `path`, `line`, `column`, `new_name` | the diff applied, from the server's rename |
| `apply_code_action` | edit | `path`, `range`, `action` (a title or id from the server's list) | the diff applied |

- `projects` follows SPEC-core's convention, with one change for workspace symbols: it defaults to the projects whose servers are **already running**, and the result lists the projects it skipped, so a query never starts every server by accident. Naming projects starts theirs (Open Question 4).
- `rename_symbol` and `apply_code_action` ask the server for an edit, then apply it with the `edit` rules of `SPEC-core.md` (inside the project's directory, content-hash check, atomic write, diff returned, touched files indexed at once). An edit that would touch a file outside the project is refused whole. A rename never reaches other projects; `dependents` (graph) tells the agent who else might break.
- In `operational-support` only the `observe` tools exist, and they see the active context's view.

## Tech Stack

- Rust 2024, `tokio` (process, io and time features), `serde`, `thiserror`.
- New dependencies, each needing approval: LSP protocol types (for example `lsp-types`), and possibly a small JSON-RPC library. The `Content-Length` framing and the request and response matching are small enough to write in-house if a library is not wanted.
- Language servers themselves are not dependencies of Workbench.

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
List:     cargo run -p workbench -- lsp list
Check:    cargo run -p workbench -- lsp check billing-api
Run:      cargo run -p workbench -- lsp run billing-api --kind python
Live:     cargo test --workspace -- --ignored    # against real language servers installed on this machine
```

## Project Structure

```
lib/engine/src/lsp/
├── kind.rs        # FileKind and detection from a path
├── catalog.rs     # the catalog: default server, command and install hint per kind
├── effective.rs   # effective settings for a project and kind (workspace precedence over the catalog)
├── route.rs       # path -> project -> kind -> effective settings
├── launch.rs      # the launch plan: working directory, command, environment (activation, venv, allow-list)
├── position.rs    # 1-based characters <-> LSP 0-based UTF-16
├── pool.rs        # the server pool: lazy start, idle shutdown, LRU eviction, restart once
└── plan.rs        # turning a server's workspace edit into an edit plan the edit tools can apply

lib/engine/src/ports.rs      # + LanguageServerLauncher (start a process), LanguageClient (send requests and notifications)

lib/adapters/src/lsp/
├── process.rs     # spawn with the launch plan, sanitised environment, link node_modules into a worktree
├── jsonrpc.rs     # Content-Length framing, request and response matching, notifications
└── client.rs      # the LanguageClient over a child process

services/cli/src/
├── commands/lsp.rs               # `workbench lsp run | list | check`
└── mcp/modules/lsp.rs            # the lsp_* tools and the two edit tools

lib/adapters/tests/fake_lsp/      # a tiny language server used by the tests
```

The engine has no I/O: the pool, routing, launch plan and positions are plain logic over the two ports.

## Code Style

The launch plan is data, built without touching the system, so it can be tested and shown by `lsp list`:

```rust
/// How to start one language server.
#[derive(Debug, PartialEq)]
pub struct LaunchPlan {
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    /// The only environment the child gets.
    pub env: BTreeMap<String, String>,
}
```

Follow the existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in `ports.rs`, adapters in `lib/adapters`.

## Testing Strategy

- **Engine unit tests:**
  - Kind detection for every row of the catalog, including Compose versus other YAML and `Dockerfile.*`.
  - Routing: longest project path wins; a path outside every project and a file of no kind both fail with a clear message.
  - Effective settings: project over workspace default over catalog, field by field, `enabled: false`, and the Angular versus Python default for `html`.
  - Launch plan: the activation prefix, the venv fallback, the working directory for a working tree and for a worktree, the allow-listed environment, and **no `*_env` secret variable ever present**.
  - Positions: 1-based characters to and from UTF-16, including emoji and combining characters.
  - Pool (with a fake launcher): lazy start, sharing between sessions, idle shutdown, LRU eviction at the limit, one restart after a crash and a report after two.
  - Edit plans: an edit outside the project is refused whole; a stale content hash is refused.
- **Adapter tests** (`lib/adapters`), against the fake language server: framing and request matching; handshake and `workspace/configuration` answered with the settings; startup and request timeouts; `didOpen` with the current disk contents and `didClose`; a crash is detected; `node_modules` linking into a worktree, and the warning when lock files differ.
- **Integration tests** (`services/cli/tests/`): `workbench lsp run` with the fake server (the working directory, the environment and the pass-through of stdin and stdout), `lsp list` and `lsp check` output, and a missing program giving the install hint.
- **Live tests** (ignored by default): real `basedpyright`, `typescript-language-server` and `yaml-language-server` on small fixture projects, including a `uv` project and an Angular project.
- Every row of the catalog, every rule under "Routing" and "Starting a server", and every tool has a test.

## Boundaries

- **Always:** pass the command as an argument list; give children the allow-listed environment only; route by project and kind; time out every startup and request; cut large results and say so; apply edits only through the `edit` rules.
- **Ask first:** adding a dependency (LSP types, a JSON-RPC library); adding a kind or changing a catalog default; installing a language server for a person; injecting settings into `lsp run`; anything that changes the `lsp` block of the workspace config.
- **Never:** run a language server through a shell; let a server inherit a variable that a Workbench config names as a secret source; install packages or run a project's own scripts; apply an edit outside a project's directory; register `rename_symbol` or `apply_code_action` outside `developer-support`; write to stdout from `lsp run` except what the server writes.

## Success Criteria

1. Every file kind in the catalog is detected correctly, and a file is routed to the project that contains it.
2. Effective settings follow project over workspace default over catalog, `enabled: false` disables a server, and `html` defaults to `ngserver` in Angular projects and to `vscode-html-language-server` otherwise.
3. A server starts in the right working directory with the project's environment, through the activation prefix or the venv fallback, and never receives a variable that a Workbench config names as a secret source.
4. `workbench lsp run <project-id> --kind python` works as an AI platform's LSP server: the server's stdin and stdout pass straight through.
5. `workbench lsp list` shows each project's servers and whether they are installed; `workbench lsp check` runs the handshake and reports `ok`, `failed` or `not found`; a missing program gives the install hint.
6. The MCP tools answer from the owning project's server; in `operational-support` they see the active context's branch and do not mix it with the working tree.
7. Servers start lazily, are shared by sessions, stop after 10 idle minutes, are evicted beyond 8, and a crash is restarted once.
8. Every startup and request has a time limit, and a timeout names the server and the request.
9. Results use 1-based paths-with-project and positions, are cut at their limits with a note, and a position with emoji or combining characters maps correctly to the server.
10. `rename_symbol` and `apply_code_action` apply through the `edit` rules, refuse an edit outside the project whole, return the diff, and exist only in `developer-support`.
11. Workbench installs nothing and runs no project script.
12. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- Injecting `lsp.settings` into `workbench lsp run` through a thin protocol proxy, which would also allow one server to serve several projects.
- Installing language servers for a person (`workbench lsp install`).
- Cross-project renames.

## Open Questions

1. **Several servers per project:** the workspace config has one `server` per project and per language, but a project needs one server per kind of file. I propose an amendment to `SPEC-workspace.md`: an `lsp` block (per project, and per language at the top level) may contain a `servers` map keyed by file kind (`python`, `typescript`, `html`, `css`, `scss`, `yaml`, `dockerfile`, `compose`), each with the usual `enabled`, `server`, `command` and `settings`; the existing fields stay as shorthand for the project's main kind. Is that right?
2. **`lsp run` is a pass-through:** it hands over stdin and stdout, so `lsp.settings` cannot be applied on that route, only on the MCP side. Injecting them needs a protocol proxy, which I put on the roadmap. Is that acceptable?
3. **Dependencies in a context worktree:** for Node projects I link the project's `node_modules` into the worktree when the lock files match, and warn when they differ. The alternative is to install packages in the worktree, which downloads files and runs install scripts, so I ruled it out. Is the link-or-warn approach right?
4. **Workspace symbol search:** it covers only servers that are already running unless you name `projects`, so a query never starts every server. Is that the right default?
