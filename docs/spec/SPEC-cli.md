# Spec: cli

Module `cli` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`workspace`](SPEC-workspace.md) (approved). The `index` and `semantic` command groups also depend on `graph` and `semantic-layer`; they are specified here as the command surface and wired once those modules exist.

## Decisions

1. The binary is `workbench` (package `workbench`, in `services/cli`).
2. This module defines the whole `workbench` command surface for managing a workspace: `init`, `project add | list | remove`, `index` and `semantic add | revise | retract | list | show | prune | generate`. What indexing and annotations *do* is specified elsewhere (`SPEC-index.md`, `SPEC-graph.md`, `SPEC-semantic-layer.md`); this spec fixes how they are invoked, scoped and reported. `workbench lsp run` (`SPEC-lsp.md`) and `workbench platform setup` (`SPEC-platform-setup.md`) have their own specs. Platform setup is its own command; `init` does not take `--platform`.
3. The CLI is a *driving adapter*: it parses arguments, calls engine use cases, and prints results. Rules live in the engine; file access lives in `lib/adapters`.
4. `.workbench/` is entirely git-ignored; `init` adds it to the workspace root's `.gitignore`.
5. `init` also generates the server config template at `.workbench/mcp/config.yml`. Its schema is defined in `SPEC-core.md`, so that step is built after `core`'s model exists.

## Further decisions

1. Commands other than `init` find the workspace by searching upward from the current directory for a `.workbench/` directory, like git does. `--workspace <DIR>` overrides the search.
2. `init` keeps `--out <DIR>` (default: the current directory) to choose where `.workbench/` is created. It no longer takes `--platform`: platform setup is its own command, `workbench platform setup`, specified in `SPEC-platform-setup.md`, and reads the workspace config that `init` creates.
3. `project add` detects language and dependency manager from files in the project, and asks for flags when it cannot decide.
4. `project remove` removes the project from the config and keeps its memory folder, because the folder may contain notes written by a person.
5. Output is plain text for people. There is no `--json` yet.
6. Commands that act on a node select it with separate flags: `--project <ID>` (the whole project), plus `--file <PATH>` (a file, relative to the project root; needs `--project`), plus `--symbol <NAME>` (a class or function by qualified name, such as `StripeGateway.charge`; needs `--file`). The target of a `link` uses `--to-project`, `--to-file` and `--to-symbol` the same way.
7. Annotation kinds are `purpose`, `role`, `domain-term`, `tag` and `link`; `SPEC-semantic-layer.md` defines them precisely.
8. LLM-assisted annotation is opt-in twice over: the workspace config must contain `semantic.llm` (`SPEC-workspace.md`), **and** the command must be given the flag. The supported providers are `copilot-cli`, `ollama` and `claude`. Only `claude` needs an API key; it is read from the environment variable named in `api_key_env`, only when a call is made, and is never stored in any config file.

## Objective

Let one person set up and maintain a workspace from the terminal in a few commands, without editing YAML by hand.

```text
workbench init                                   # create .workbench/ here
workbench project add services/billing           # detect python + uv, create its memory
workbench project add ../shared/web --id web     # detect angular + pnpm
workbench project list
workbench index                                  # parse changed files in every project
workbench semantic add --project billing-api --file src/payments/gateway.py \
    --symbol StripeGateway --kind purpose \
    --text "Adapter to the Stripe API; the only place that calls it"
workbench project remove web
```

The module is done when those commands work end to end on a real mono repo and on two separate repos, and every failure tells the person what to do next.

## Commands

### `workbench init [--out <DIR>]`

Creates, under `<DIR>` (default `.`):

| Created | Content |
|---|---|
| `.workbench/config.yml` | `version: 1`, `projects: []` (the format in `SPEC-workspace.md`) |
| `.workbench/mcp/config.yml` | The server config template (`SPEC-core.md`); added once `core` defines it |
| `<DIR>/.gitignore` | A `.workbench/` line appended; the file is created if missing; nothing is added twice |

Refuses to run if `.workbench/config.yml` already exists, and changes nothing in that case. `init` does not set up any AI platform; run `workbench platform setup` afterwards.

### `workbench platform setup [--platform <P>[,<P>...]]`

Defined in `SPEC-platform-setup.md`. Listed here so the command surface is in one place: it configures the AI platform's own LSP integration (default platforms: `copilot-cli,claude-code`) from the workspace config, so it needs a workspace and fails with the usual `no .workbench/ found` error otherwise.

### `workbench project add <PATH> [options]`

| Option | Meaning |
|---|---|
| `--id <ID>` | Project id. Default: the directory name as kebab-case. |
| `--language <python\|angular>` | Default: detected. |
| `--dependency-manager <poetry\|uv\|npm\|pnpm\|yarn\|bun>` | Default: detected. |
| `--venv <PATH>` | Python only. Default: `.venv` if it exists in the project. |
| `--activate <COMMAND>` | Command prefix to launch tools in the project. Default: derived from the dependency manager (`uv run`, `poetry run`), none for Angular. |
| `--index` | Index the new project right after adding it (same as running `workbench index --project <id>`). A failed index does not undo the add. |

Steps, all or nothing:
1. Resolve and validate the path (exists, is a directory).
2. Detect language and dependency manager, unless given.
3. Build the project and validate it against the workspace rules (unique id, valid pair, and so on).
4. Create `.workbench/memory/<id>/INIT-MEMORY.md`, seeded as in `SPEC-workspace.md`.
5. Save the config.

If any step fails, the config and the memory folder are left as they were.

**Detection rules** (read files only, never run project code):

| Signal | Result |
|---|---|
| `angular.json` present | language `angular` |
| `pyproject.toml` present | language `python` |
| `uv.lock` or a `[tool.uv]` table | dependency manager `uv` |
| `poetry.lock` or a `[tool.poetry]` table | dependency manager `poetry` |
| `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `bun.lock` or `bun.lockb` | `pnpm`, `yarn`, `npm`, `bun` |

Both languages detected, both Python managers detected, or none detected: the command fails and names the flag to pass.

### `workbench project list`

Prints one line per project: id, language, dependency manager, path. Prints a short hint when there are none.

### `workbench project remove <ID>`

Removes the project from the config. Keeps `.workbench/memory/<id>/`. `--purge-memory` deletes that folder too, after printing what it will delete. An unknown id fails and lists the known ids.

### `workbench index [options]`

Builds the AST data for the projects in the workspace and keeps it up to date. It is **incremental by default**: only files that changed since the last run are re-parsed, and what depends on them is re-linked. How that works is specified in `SPEC-index.md`.

| Option | Meaning |
|---|---|
| `--project <ID>` | Limit to this project. Repeatable. Default: every project. An unknown id fails and lists the known ones. |
| `--changed` | Only consider files that git reports as changed. Faster on large repos; falls back to content hashes where a project is not a git repo. |
| `--full` | Re-parse every file in scope, ignoring what is already indexed. |
| `--annotate` | After indexing, ask the configured LLM to draft annotations for the nodes that are new or changed and have none. Same behaviour as `semantic generate` on those nodes. Off by default. |

- Prints progress to stderr and a summary to stdout, per project: files parsed, unchanged, removed, nodes re-linked, parse failures, and the number of annotations that became stale.
- Fails with a hint when the workspace has no projects.
- It reads project files and writes only to the storage backend; it never changes a project's files.
- The MCP `reindex` tool runs the same use case, so both produce the same result.
- Exit codes follow "Common behaviour"; how partial failures (some files fail to parse) are counted is defined in `SPEC-index.md`.

### `workbench semantic <command>`

Adds meaning to the indexed AST. Commands that act on a node select it with the flags in Further decision 6.

| Command | Meaning |
|---|---|
| `add --project <ID> [--file <PATH> [--symbol <NAME>]] --kind <KIND> --text <TEXT> [--to-project <ID> [--to-file <PATH> [--to-symbol <NAME>]]]` | Attach an annotation. The `--to-*` flags are required for `--kind link` and rejected otherwise. |
| `revise <ID> [--text <TEXT>]` | Replace the text of an annotation. Without `--text`, confirm that the annotation still holds and re-anchor it to the node's current version, which clears its stale flag. |
| `retract <ID>` | Remove one annotation. |
| `prune [--project <ID>] [--stale] [--orphaned] [--dry-run] [--yes]` | Remove stale annotations (their node changed) and orphaned entries (their node disappeared, kept as a tombstone so the annotation stayed visible). With neither `--stale` nor `--orphaned`, both. Lists what it will remove and asks to confirm; `--yes` skips the question, and is required when not run in a terminal. `--dry-run` only lists. |
| `generate [--project <ID> [--file <PATH> [--symbol <NAME>]]] [--max-nodes <N>] [--dry-run]` | Ask the configured LLM to draft annotations for nodes that have none. Needs `semantic.llm` in the workspace config. Off unless this command (or `index --annotate`) is run. `--dry-run` prints how many nodes and roughly how much text would be sent, and makes no call. |
| `list [--project <ID> [--file <PATH> [--symbol <NAME>]]] [--kind <KIND>] [--stale] [--orphaned]` | List annotations, optionally only stale or orphaned ones. |
| `show --project <ID> [--file <PATH> [--symbol <NAME>]]` | Show a node with its annotations, its neighbours in the graph, and whether each annotation is current or stale. |

- The target must exist in the index. If not, the command fails and suggests `workbench index`.
- Annotations created here are recorded as written by a person. An agent cannot overwrite or remove them.
- Annotations are stored in the storage backend through the `semantic-layer`. Those written by a person (`add`, `revise`, `retract`, `prune`) are also mirrored to `.workbench/memory/<project-id>/annotations.yml`, so they survive if the backend is wiped; agent-written ones are not mirrored. Nothing is written into the project's source files.
- **LLM drafting** (`generate`, `index --annotate`):
  - Fails before any call if `semantic.llm` is missing, if `claude` is chosen and its environment variable is unset, if `copilot-cli` is chosen and the `copilot` program cannot be found, or if `ollama` is chosen and its endpoint cannot be reached; the error says which.
  - Sends only the source of indexed nodes (signature, body and nearby context, defined in `SPEC-semantic-layer.md`), never other files.
  - Targets only nodes with no annotation, or whose only annotations are stale ones written by an agent. It never overwrites or removes a person-written annotation.
  - Records each result as written by an agent, with the model name, so people can review and `prune` or `retract` them.
  - `--max-nodes <N>` caps how many nodes one run sends. There is no cap unless it is given, so run `--dry-run` first on a large workspace.
  - Prints what it sent (node count, provider, model) and what it created.

### Common behaviour

| Topic | Rule |
|---|---|
| Exit codes | `0` success; `1` any failure; `2` bad usage (clap) |
| Errors | Written to stderr as `Error: <what happened>. <what to do>` |
| Workspace not found | `Error: no .workbench/ found from <cwd>. Run 'workbench init'.` |
| Help | `--help` on every command; the banner shows on the top-level help only |

## Tech Stack

- Rust 2024; `clap` (derive) for arguments; engine and adapters from the workspace.
- `serde` and `yaml_serde` come in through `lib/adapters`; the CLI does not depend on them directly.
- No new dependencies. Integration tests use the binary path cargo provides to tests (`env!("CARGO_BIN_EXE_workbench")`) and `std::process::Command`.

## Project Structure

```
services/cli/
├── src/
│   ├── main.rs        # wiring only: parse, build adapters, call engine, print
│   ├── cli.rs         # clap types (driving adapter): Cli, Command, ProjectCommand, IndexArgs, SemanticCommand
│   ├── output.rs      # formatting of results and errors
│   └── utils/         # banner
└── tests/
    └── cli.rs         # runs the built binary against temp directories

lib/engine/src/
├── init.rs            # init_workspace use case (extended)
└── workspace/
    └── ...            # add_project, remove_project, list_projects use cases

lib/engine/src/ports.rs   # + ProjectInspector, GitignoreStore, ServerConfigStore
lib/adapters/src/         # + project_inspector.rs, gitignore_file.rs, server_config_file.rs
```

### Ports added by this module

| Port | Responsibility |
|---|---|
| `ProjectInspector` | `inspect(path)` returns what is detectable: language, dependency manager, venv. Reads files only. |
| `GitignoreStore` | `ensure_ignored(root, entry)` appends the entry once, creating the file if missing. |
| `ServerConfigStore` | `create_template(root)` writes `.workbench/mcp/config.yml`, never overwriting. |

`WorkspaceStore`, `MemoryStore` and `ContextSource` come from `SPEC-workspace.md`. The `index` and `semantic` commands call engine use cases (`index_projects`, and the annotation use cases) that are defined by `SPEC-index.md` and `SPEC-semantic-layer.md`; the CLI adds no rules of its own for them.

## Code Style

`main.rs` stays a thin wiring layer; use cases return data and the CLI decides how to print it.

```rust
fn run(cli: Cli) -> Result<(), engine::Error> {
    let fs = adapters::Fs::default();
    match cli.command {
        Command::Project(ProjectCommand::List) => {
            let workspace = locate_workspace(cli.workspace)?;
            for p in engine::list_projects(&fs.workspace(&workspace))? {
                println!("{}\t{}\t{}\t{}", p.id, p.language, p.dependency_manager, p.path.display());
            }
        }
        // ...
    }
    Ok(())
}
```

Follow the existing conventions: doc comments on every public item, no `unwrap` or `panic`, a `Platform`-style mapping type (`PlatformArg`) for every enum clap needs to parse.

## Testing Strategy

- **Engine unit tests** with fake ports: `add_project` rolls back on failure, a duplicate id is rejected, `remove_project` keeps memory by default, `init` is refused when a config exists.
- **Adapter tests** (in `lib/adapters`): detection against fixture directories (uv, poetry, angular with each lock file, ambiguous, empty); `.gitignore` handling (missing file, existing entry, existing file without a trailing newline).
- **Integration tests** (`services/cli/tests/cli.rs`): run the real binary in a temp directory through the sequence `init`, `project add`, `list`, `remove`, and check the files and exit codes.
- **LLM drafting and prune:** with a fake `LlmClient` port: no call is made without the flag, without `semantic.llm`, or with `--dry-run`; the missing-variable error names the variable; a person-written annotation is never overwritten; `prune --dry-run` removes nothing; `prune` without `--yes` asks first and refuses to run non-interactively; `prune` removes exactly the stale and orphaned entries it listed.
- **Index and semantic commands:** engine tests with fake ports check that `--project`, `--changed` and `--full` map to the right request, that an unknown project or target fails with a hint, and that a `link` without `--to` is rejected. End-to-end runs of both groups are part of the integration test once `graph` and `semantic-layer` exist.
- **Node flags:** `--file` without `--project`, `--symbol` without `--file`, and `link` without `--to-project` are rejected by argument parsing with a clear message.
- Every row of the detection table and every error message in "Common behaviour" has a test.

## Boundaries

- **Always:** change nothing on failure; print what was created or changed; validate with the engine, not in the CLI.
- **Ask first:** adding a dependency (for example `assert_cmd`, a temp-directory crate, or the HTTP client for the `claude` and `ollama` adapters); running an external program for the `copilot-cli` provider; a `--force` flag; any command that writes outside the workspace root or the project paths; a `--json` output mode.
- **Never:** call an LLM without the explicit flag and a `semantic.llm` config; send anything other than indexed source nodes; print or log the API key; delete a person-written annotation without the person running `retract` or `prune`; modify a project's source files (indexing and annotating read them only); install git hooks or file watchers (those belong to `SPEC-index.md`); run project code or package managers during detection; read secrets; overwrite `INIT-MEMORY.md`; delete a memory folder without `--purge-memory`.

## Success Criteria

1. `workbench init` creates `.workbench/config.yml` and a `.gitignore` entry, and a second run fails with no changes.
2. `project add` on a uv project, a poetry project and an Angular project (each package manager) detects correctly and creates the memory folder.
3. An ambiguous or undetectable project fails and names the flag to pass.
4. A failure in any step of `project add` leaves the config and memory unchanged.
5. `project list` and `project remove` behave as described, and `remove` keeps the memory folder unless `--purge-memory` is given.
6. From a subdirectory, commands find the workspace; outside one they print the `no .workbench/ found` error.
7. `workbench index` indexes all projects or the chosen ones, a second run parses nothing that has not changed, `--full` re-parses everything, and an unknown project fails with the list of known ids.
8. `workbench semantic add` attaches an annotation to an indexed node; `list --stale` shows annotations whose node changed after the next `index`; `retract` removes one; a target that is not indexed fails with a hint. A person-written annotation also appears in `.workbench/memory/<id>/annotations.yml`; an agent-written one does not.
9. `workbench semantic prune` lists and then removes exactly the stale and orphaned entries in scope, does nothing under `--dry-run`, and refuses to run non-interactively without `--yes`.
10. `workbench semantic generate` and `index --annotate` call the LLM only when both the flag and `semantic.llm` are present, record their output as agent-written, and never touch a person-written annotation; `--dry-run` makes no call.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench project scan <DIR>`: find the projects under a directory and add them in one go. Projects are added one at a time for now.

## Open Questions

None. The questions from the first drafts are answered and folded into the sections above.
