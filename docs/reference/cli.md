# CLI reference: `workbench`

One page listing every `workbench` command, with its options and the spec that defines it. It is a summary: **the spec wins** where they differ. Planned commands are marked, and nothing here is built yet beyond the scaffold.

Status key: **A** = build Slice A (first slice, [plan](../../tasks/plan.md)); **wired** = spec approved, built as its module is built; **later** = needs a spec that is not written yet.

## Command index

| Command | Purpose | Defined in | Status |
|---|---|---|---|
| `workbench init` | Create `.workbench/` in a directory | `SPEC-cli` | A |
| `workbench project add` | Add a project to the workspace | `SPEC-cli` | A |
| `workbench project list` | List the workspace's projects | `SPEC-cli` | A |
| `workbench project remove` | Remove a project | `SPEC-cli` | A |
| `workbench storage status` | Check Memgraph and what is stored | `SPEC-cli`, `SPEC-storage` | wired (`storage`) |
| `workbench mcp start \| serve \| stop \| restart \| status \| logs \| connect` | Run and manage the MCP server | `SPEC-core` | wired (`core`) |
| `workbench context check` | Check a context's connections | `SPEC-cli`, `SPEC-contexts` | later (its `code` line needs `graph`) |
| `workbench context sync` | Create or move a context's branch worktrees | `SPEC-cli`, `SPEC-contexts` | later (indexes the view, needs `graph`) |
| `workbench index` | Build and update the AST data | `SPEC-cli` (`SPEC-index` to write) | later |
| `workbench watch [start] \| stop \| status \| logs \| flush` | Keep the index fresh | `SPEC-cli` (`SPEC-index` to write) | later |
| `workbench semantic add \| revise \| retract \| prune \| generate \| list \| show` | Annotations on AST nodes | `SPEC-cli` (`SPEC-semantic-layer` to write) | later |
| `workbench platform setup` | Configure an AI platform's LSP integration | `SPEC-platform-setup` (to write) | later |
| `workbench platform hooks install \| uninstall \| list` | Install platform hooks for `watch` | `SPEC-platform-setup` (to write) | later |
| `workbench lsp run <project-id>` | Launch a project's language server over stdio | `SPEC-lsp` (to write) | later |

## Common behaviour

| Topic | Rule |
|---|---|
| Workspace discovery | Every command except `init` searches upward from the current directory for `.workbench/`, like git. `--workspace <DIR>` overrides the search and names the workspace root. |
| Exit codes | `0` success; `1` any failure; `2` bad usage (clap) |
| Errors | stderr, as `Error: <what happened>. <what to do>` |
| Workspace not found | `Error: no .workbench/ found from <cwd>. Run 'workbench init'.` |
| Output | Plain text for people; no `--json` yet. Progress and diagnostics go to stderr, results to stdout |
| Help | `--help` on every command; the banner shows on the top-level help only |
| Failure safety | A failed command changes nothing (or, for `project add`, removes what that run created); existing files are never overwritten or deleted |

## `init`

```
workbench init [--out <DIR>]
```

| Option | Meaning |
|---|---|
| `--out <DIR>` | Where to create `.workbench/`. Default `.` |

Creates `.workbench/config.yml` (`version: 1`, a `storage` block with `localhost`, `7687` and a unique `namespace`, `projects: []`), `.workbench/memory/INIT-MEMORY.md`, and a `.workbench/` line in `<DIR>/.gitignore` (created if missing, never added twice). Refuses, changing nothing, if `config.yml` exists. No `--platform`: use `platform setup`.

The server config template (`.workbench/mcp/config.yml`) and one context template per default context are added when `core` is built; they are not part of Slice A.

## `project`

### `project add <PATH>`

| Option | Meaning |
|---|---|
| `--id <ID>` | Project id (`[a-z][a-z0-9-]{0,63}`). Default: derived from the directory name; fails naming `--id` if that is not valid |
| `--language <python\|typescript>` | Default: detected |
| `--framework <angular>` | Optional; only with `typescript`. Default: detected |
| `--dependency-manager <poetry\|uv\|npm\|pnpm\|yarn\|bun>` | Default: detected. Must pair with the language |
| `--venv <PATH>` | Python only. Default: `.venv` if present |
| `--activate <COMMAND>` | Command prefix to launch tools in the project. Default: `uv run`, `poetry run`, or none |
| `--compose <PATH>` | Compose file, relative to the project. Default: first of `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml` at the project root. Error if given and missing |
| `--index` | Index right after adding (needs `index`; rejected as unknown in Slice A) |

All or nothing: validates the path, detects language and manager (reads files only), validates against the workspace rules, creates `.workbench/memory/<id>/INIT-MEMORY.md` (seeded from `README.md`, `CLAUDE.md`, `AGENTS.md` if present) and an empty `annotations.yml`, then saves the config. Existing memory files are never overwritten. Ambiguous or undetectable language or manager fails naming the flag to pass. Binaries are not set here; they are written by hand in `config.yml`.

Detection: `angular.json` → `typescript` + `angular`; `pyproject.toml` → `python`; `uv.lock` or `[tool.uv]` → `uv`; `poetry.lock` or `[tool.poetry]` → `poetry`; `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `bun.lock`/`bun.lockb` → `pnpm`, `yarn`, `npm`, `bun`.

### `project list`

One line per project: id, language (`typescript/angular` when a framework is set), dependency manager, path. A hint when empty.

### `project remove <ID>`

| Option | Meaning |
|---|---|
| `--purge-memory` | Also delete `.workbench/memory/<id>/`, after printing what will be deleted |

Removes the entry and keeps the memory folder, so re-adding finds both files. Works when the project directory is gone. An unknown id fails and lists the known ids. Indexed data is removed by the next `index` that covers every project.

## `storage status`

```
workbench storage status
```

Read-only. Prints Memgraph's `host:port` and namespace, whether it answers and how fast, each module's schema version, and per view the counts of files, nodes, edges and annotations. Needs only the workspace config. Applies no migrations. If unreachable: exit `1`, names host and port, hints `docker run -p 127.0.0.1:7687:7687 memgraph/memgraph`, no secret in the message.

## `mcp`

All take `--workspace <DIR>` and, where a mode applies, `--mode <MODE>`: `developer-support` (default), `operational-support`, `operational-onboarding` and `developer-onboarding` (planned).

| Command | Behaviour |
|---|---|
| `mcp start [--mode M] [--context C]` | Start the server in the background and wait until ready or failed. For `operational-support` it first runs the context check for `C` (default `default_context`) and refuses to start if it fails. `--context` is rejected for other modes. Prints pid, socket and log path. Fails if one is already running |
| `mcp serve [--mode M]` | Run in the foreground, logging to stderr |
| `mcp stop [--mode M \| --all]` | Clean shutdown; shims keep waiting |
| `mcp restart [--mode M]` | `stop` then `start`; reloads configs; shims reconnect (up to 90 s) |
| `mcp status [--mode M]` | Per server: mode, pid, uptime, socket, clients, log path; removes stale pid files |
| `mcp logs [--mode M] [--session ID [--raw] \| --sessions] [-f] [-n N]` | Last `N` lines (default 50) of the latest server log; `-f` follows (Ctrl-C exits `0`); `--sessions` lists session logs; `--session ID` renders one (`--raw` for JSON Lines). Session logs exist for `operational-support` only |
| `mcp connect [--mode M] [--start]` | stdio shim for AI platforms. Writes only MCP messages to stdout. With no server: exits with `no <mode> server running; run 'workbench mcp start --mode <mode>'`. `--start` is allowed for `developer-support` only |

## `context` (operational-support)

| Command | Behaviour |
|---|---|
| `context check [--context NAME]` | One line per system (`ssh`, `kubectl`, `postgres`, `redis`, `kafka`, `code`): `ok`, `failed` or `skipped`, with timing. Needs the server config and the context file, both complete. Checks only that context. Exit `0` if all checked are `ok`, else `1`. Does not verify credentials are read-only |
| `context sync [--context NAME] [--project ID]... [--fetch]` | Create or move one git worktree per project at the context's branch (`.workbench/worktrees/<context>/<project-id>/`) and index it. Uses the local branch unless `--fetch`. Never touches a project's own working tree, branch or refs. `--context` defaults to `default_context` |

## `index` (later)

```
workbench index [--project <ID>]... [--changed | --full] [--annotate]
```

| Option | Meaning |
|---|---|
| `--project <ID>` | Limit to a project; repeatable. Unknown id fails and lists known ids |
| `--changed` | Only files git reports as changed (falls back to hashes outside git) |
| `--full` | Re-parse everything. With `--changed`: exit `2` |
| `--annotate` | Then ask the configured LLM to draft annotations for new or changed nodes without one. Needs `semantic.llm` |

Incremental by default; reads project files, writes only to Memgraph; progress on stderr, per-project summary on stdout. Fails with a hint when the workspace has no projects. Shares one lock with `watch` and the MCP `reindex` tool.

## `watch` (later)

| Command | Behaviour |
|---|---|
| `watch [start] [--project ID]...` | Start the detached watcher; prints pid and log path; safe to repeat (exit `0` if already running) |
| `watch stop` | Stop cleanly; exit `0` if none runs |
| `watch status` | Running or not, pid, uptime, projects, last index run, files indexed, pending changes, last error |
| `watch logs [-f] [-n N]` | Print or follow the watcher log; works after it stopped |
| `watch flush` | Index pending changes now and wait; runs one incremental index itself if no watcher runs |

Batches events (500 ms). Needs the workspace config and Memgraph only. Never calls an LLM or changes project files. Log: `.workbench/logs/watch-<YYYYMMDD-HHMMSS>.log`.

## `semantic` (later)

Node selection flags (any command that targets a node): `--project <ID>`, `--file <PATH>` (needs `--project`), `--symbol <NAME>` (needs `--file`). A link's target uses `--to-project`, `--to-file`, `--to-symbol`, or `--to-custom <NAME>`. Kinds: `purpose`, `role`, `domain-term`, `tag`, `link`.

| Command | Meaning |
|---|---|
| `semantic add --project ID [--file PATH [--symbol NAME]] --kind KIND --text TEXT [--to-project ID [--to-file PATH [--to-symbol NAME]] \| --to-custom NAME] [--context NAME]...` | Attach an annotation. `link` needs one target and at least one `--context` (`working`, `production`, `staging`, `testing`); both are rejected for other kinds |
| `semantic revise <ID> [--text TEXT]` | Replace the text; without `--text`, confirm and re-anchor (clears stale) |
| `semantic retract <ID>` | Remove one annotation |
| `semantic prune [--project ID] [--stale] [--orphaned] [--dry-run] [--yes]` | Remove stale annotations and orphaned tombstones (both if neither flag). Confirms unless `--yes` (required outside a terminal) |
| `semantic generate [--project ID [--file PATH [--symbol NAME]]] [--max-nodes N] [--dry-run]` | LLM-draft annotations for nodes with none. Needs `semantic.llm`. `--dry-run` makes no call |
| `semantic list [--project ID [--file PATH [--symbol NAME]]] [--kind KIND] [--context NAME] [--stale] [--orphaned]` | List annotations |
| `semantic show --project ID [--file PATH [--symbol NAME]]` | A node, its annotations (current or stale) and its graph neighbours |

Person-written annotations are mirrored to `.workbench/memory/<project-id>/annotations.yml` and never overwritten by an agent. LLM use is opt-in twice: `semantic.llm` in the config **and** `generate` / `index --annotate`.

## `platform` and `lsp` (later, specs not written)

| Command | Behaviour (from the capability map and `SPEC-cli`) |
|---|---|
| `platform setup [--platform P[,P...]]` | Configure the AI platform's own LSP integration from the workspace config. Default platforms `copilot-cli,claude-code`. Never installs hooks |
| `platform hooks install \| uninstall \| list [--platform P[,P...]]` | Install, remove or show the platform hooks that run `watch` at session start, `watch flush` after an edit, `watch stop` at session end. Only when asked |
| `lsp run <project-id>` | Run the project's activation command and exec its language server over stdio. Used by platform LSP config |

## Not planned yet (roadmap)

`project scan <DIR>`, `context clean`, `mcp logs --prune`, `mcp start --http`, a `--json` output mode.
