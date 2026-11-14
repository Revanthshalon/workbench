# Workbench

Workbench is a command-line tool that sets up a **workspace** of projects and
records what the workspace contains. Each workspace keeps its state in a
`.workbench/` directory at its root.

## Requirements

- Rust (the toolchain pinned in `rust-toolchain.toml`)
- cmake and OpenSSL 3, for the Memgraph client built by default
  (`brew install cmake openssl@3` on macOS)
- A running [Memgraph](https://memgraph.com) for `workbench index`, by default on
  `localhost:7687`

## Build

```sh
cargo build --release
./target/release/workbench --help
```

`cargo build --no-default-features` builds without the Memgraph client (no cmake or
OpenSSL needed); `index` then stores nothing.

## Commands

### `workbench init`

Creates the `.workbench/` layout in a workspace root.

```sh
workbench init                  # current directory
workbench init --out ../billing # another directory
workbench init --force          # rewrite config files to their defaults
```

| Option | Meaning |
|---|---|
| `--out <DIR>` | Workspace root in which `.workbench/` is created. Default: `.` |
| `--force` | Rewrite config files to their defaults. Without it, existing files are kept. |

Each file is reported as `created`, `exists`, `rewrote` or `updated`.

`init` is safe to run again. It keeps existing files and creates any that are
missing. `--force` rewrites:

- `.workbench/config.yml`
- `.workbench/mcp/config.yml`
- `.workbench/mcp/contexts/{production,staging,testing}.yml`

A forced rewrite of `config.yml` keeps the existing `storage.namespace`, so the
workspace stays attached to the data stored under it. `--force` never rewrites
`.workbench/memory/INIT-MEMORY.md`, because it holds hand-written notes.

`init` also adds `.workbench/` to the `.gitignore` in the workspace root, once.

### `workbench project add <PATH>`

Registers a project in the workspace. The project directory is `<PATH>`,
relative to the current directory or absolute. The command finds the workspace
by looking for `.workbench/config.yml` in the current directory and then in its
parents, so run it inside the workspace.

```sh
workbench project add services/billing
workbench project add ../shared/web --id web
```

| Option | Meaning |
|---|---|
| `--id <ID>` | Project id. Default: the directory name, lower-cased, with non-alphanumeric runs replaced by `-`. |
| `--language <python\|typescript>` | Default: detected. |
| `--framework <angular>` | TypeScript only. Default: detected from `angular.json`. |
| `--dependency-manager <poetry\|uv\|npm\|pnpm\|yarn\|bun>` | Default: detected. |
| `--venv <PATH>` | Python only. Default: `.venv` if it exists in the project. |
| `--activate <COMMAND>` | Command prefix for running tools in the project. Default: `uv run` or `poetry run` for Python, none for TypeScript. |
| `--compose <PATH>` | Compose file, relative to the project. Default: the first of `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml` at the project root. |
| `--index` | Index the new project right after adding it, like `workbench index --id <ID>`. A failed index does not undo the add. |

**Detection** looks at files inside the project directory:

- **Language**: `pyproject.toml`, `requirements.txt` or `setup.py` means Python.
  `tsconfig.json`, `package.json` or `angular.json` means TypeScript. If both
  kinds of file are present, or neither, pass `--language`.
- **Dependency manager**: from the lockfile (`uv.lock`, `poetry.lock`,
  `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`/`bun.lockb`, `package-lock.json`).
  For Python without a lockfile, the `[tool.uv]` or `[tool.poetry]` table in
  `pyproject.toml` is used.

**Combinations** are checked when the flags are read. These are rejected:

- `--framework` with Python
- `--venv` with TypeScript
- a dependency manager from the other language, such as `--dependency-manager uv`
  with TypeScript

After adding, the command prints the project's fields in the same table style as
`workbench project list`.

A project id must be kebab-case and start with a letter. If the id derived from
the directory name is invalid, the command fails and asks for `--id`. A project
id or path that is already in the workspace is rejected.

Each project is written to the `projects:` list in `.workbench/config.yml`, and
gets a memory folder at `.workbench/memory/<id>/` with an `INIT-MEMORY.md`
template and an empty `annotations.yml`. Files already in that folder are kept,
so a project removed and added again under the same id keeps its notes.
Paths are stored relative to the workspace root. Comments and layout in the file
are preserved, but a `projects:` list written inline (`projects: [...]`) is
rejected and must be converted to block form first.

### `workbench project list`

Prints the projects as a table: id, language, dependency manager and path. The
language is written as `typescript/angular` when a framework is set, and a field
missing from the config is shown as `-`. Like `project add`, it finds the
workspace from the current directory or its parents.

```
$ workbench project list
+-------------+--------------------+--------------------+------------------+
| ID          | LANGUAGE           | DEPENDENCY MANAGER | PATH             |
+-------------+--------------------+--------------------+------------------+
| billing-api | python             | uv                 | services/billing |
| web         | typescript/angular | pnpm               | ../shared/web    |
+-------------+--------------------+--------------------+------------------+
2 projects
```

In a terminal the table is coloured; colours are left out when the output is
piped or redirected.

With no projects it prints a hint to run `workbench project add <PATH>`.

### `workbench project remove <ID>`

Removes a project from `.workbench/config.yml`. Comments and layout in the file
are preserved, and removing the last project leaves `projects: []`.

```sh
workbench project remove web
workbench project remove web --purge-memory
```

| Option | Meaning |
|---|---|
| `--purge-memory` | Also delete the project's memory folder, `.workbench/memory/<ID>/`. |
| `-y`, `--yes` | Delete without asking for confirmation. |

- **Memory is kept by default.** `.workbench/memory/<ID>/` (`INIT-MEMORY.md` and
  `annotations.yml`) stays, so adding the project again under the same id finds
  both.
- **`--purge-memory`** first prints a table of the files it will delete, then asks
  for confirmation (default: no). Declining changes nothing, including the config.
  Without a terminal, the command fails instead of deleting; pass `--yes` in
  scripts.
- **Unknown ids** fail, and the error lists the ids the workspace knows.
- **Indexed data** for the project is removed by the next `workbench index` that
  covers every project. That command is not implemented yet.

### `workbench project cleanup`

Deletes orphaned memory folders: folders in `.workbench/memory/` whose name is not
the id of a project in the config, such as those `project remove` kept.

```sh
workbench project cleanup
workbench project cleanup --yes
```

| Option | Meaning |
|---|---|
| `-y`, `--yes` | Delete without asking for confirmation. |

It prints a table of the folders with their file counts and sizes, then asks for
confirmation (default: no). Declining deletes nothing, and without a terminal the
command fails unless `--yes` is given. The config is never changed. Files directly
in `.workbench/memory/`, such as the workspace `INIT-MEMORY.md`, and symbolic links
are never deleted. With nothing to delete, it says so without asking.

### `workbench index`

Reads the projects' files and writes the index to storage: Memgraph, configured in
the `storage` block of `.workbench/config.yml`. It never changes a project's files.

> **Not yet:** references are stored unresolved (the link step comes next), so
> `re-linked`, `stale` and `orphaned` stay at 0. TypeScript files are only checked
> to be valid UTF-8 text and store just their module node.

```sh
workbench index                # every project
workbench index --id api       # one project
workbench index --full         # re-parse every file
workbench index --changed      # only files git reports as changed
workbench index --force        # also parse files over 1 MiB
workbench index --log-graph    # write each parsed file's graph to the log
```

| Option | Meaning |
|---|---|
| `--id <ID>` | Index only this project. Default: every project. |
| `--full` | Re-parse every file, ignoring stored content hashes. Deleted files are still removed. |
| `--changed` | Only files git reports as modified, staged, deleted or untracked since the last commit. Cannot be combined with `--full`. |
| `--force` | Parse files of any size. Without it, files over 1 MiB are skipped. Symbolic links are always skipped. |
| `--log-graph` | Write every parsed file's graph (its nodes, edges and references) to `.workbench/logs/workbench-YYYY-MM-DD.log`, for checking what the parser produced. Unchanged files are not parsed, so they are not logged. |

Progress goes to stderr and the summary to stdout. The summary is a table with one
row per project: status, files parsed, unchanged, removed, skipped, partial and
failed, nodes re-linked, and annotations that became stale or orphaned.

**Which files are indexed:**

- Python: `.py`, `.pyi`.
- TypeScript: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`.
- These directories are never entered: hidden directories (such as `.git` and
  `.venv`), `node_modules`, `__pycache__`, `venv`, `dist`, `build`, `coverage`,
  `target`, and the project's configured venv.
- Symbolic links are skipped, and so are files over 1 MiB unless `--force` is given.

**Python parsing** (tree-sitter) produces, per file:

- a node for the module, each class, each function and each method, identified as
  `<project-id>:<qualified name>` (for example `billing-api:billing.invoices.Invoice.total`;
  a leading `src/` folder is not part of the module name unless `src/` has an
  `__init__.py`, and `.pyi` stub ids end in `@pyi`);
- a `Variable` node for each name assigned at module level (`settings = …`,
  `a, b = …`, `x: T`), at its first assignment; a class or function of the same
  name takes precedence;
- `Contains` edges from each scope to what is defined directly inside it;
- `Imports` and `Inherits` references to dotted names. Relative imports resolve
  against the file's package, and base classes resolve to the binding in effect
  where the class is defined: the last import, class or variable of that name before
  it, in its own scope, enclosing functions, then the module.

When two files of a project give the same module name (`pkg/x.py` and
`pkg/x/__init__.py`, or `x.py` and `src/x.py`), the file Python would load keeps the
id and every id from the others ends in `@<path>`. The precedence is: a package's
`__init__` first, then a file under a plain `src/`, then the earlier file extension,
then the path. Each collision is logged.

A file with syntax errors is counted as `partial`: everything outside the broken
parts is kept, and the log names the lines.

**Exit code** is 1 if any file could not be read or parsed, a project could not be
written or indexed, the lock could not be taken, or the run was interrupted
(Ctrl-C). Skipped and partial files are not failures.

Only one run at a time may index a workspace. A run holds `.workbench/index.lock`
and deletes it when it finishes, including when interrupted. If a run was killed
and left the lock behind, delete the file.

`workbench project add --index` runs the same indexing for the new project. A
failed index does not undo the add.

**What is stored.** Every node carries the workspace's `storage.namespace`, and every
query is scoped to it, so several workspaces can share one Memgraph.

| Node label | Properties |
|---|---|
| `Workspace` | `namespace` |
| `Project` | `id`, `path`, `language`, `framework` |
| `File` | `project`, `path`, `hash` (16 hex digits) |
| `Code` plus `Module`, `Class`, `Function`, `Method` or `Variable` | `project`, `id`, `kind`, `name`, `path`, `start_line`, `end_line` |
| `Reference` | `project`, `path`, `target`, `line` |

`CONTAINS` links the workspace to its projects, a project to its files, a file to its
module node, and each scope to what is defined directly inside it. Each import or
base class is an `IMPORTS` or `INHERITS` relationship (with its `line`) from the
code node to its own `Reference` node, whose `target` is the dotted name. For example,
the classes inheriting from a base:

```cypher
MATCH (c:Class {namespace: $ns})-[:INHERITS]->(r:Reference {target: 'src.models.main_model.MainModel'})
RETURN c.id, c.path, c.start_line
```

**Incremental runs.** A file is skipped as `unchanged` when its stored hash still
matches: the hash covers its contents and how it is named. Files that disappeared are
`removed` from Memgraph. Each project is written in one transaction: if writing
fails, nothing of it changes and the project shows `write failed`.

**Removed projects.** A run that covers every project (no `--id`) and is not
interrupted deletes the stored data of projects no longer in the config, and names
them in the summary.

**Connecting.** `index` connects before reading any file. If Memgraph cannot be
reached, it fails at once with `Could not connect to Memgraph at <host>:<port>. Is it
running?`, and the details go to the log. The `storage` block can also name a
`username` and `password_env`, the name of the environment variable holding the
password (never the password itself).

## Errors and logs

The terminal shows a short message for every error. The details, such as the
file, the operating-system error and the full error, are appended to
`.workbench/logs/workbench-YYYY-MM-DD.log`, one file per day in UTC. The error
message ends with the log's path. Outside a workspace there is no log.

Exit codes: 0 on success, 1 when a command fails, 2 for invalid usage.

## Workspace layout

```
<workspace>/
  .gitignore                       # contains .workbench/
  .workbench/
    config.yml                     # version, storage and projects
    index.lock                     # only while `workbench index` runs
    logs/
      workbench-YYYY-MM-DD.log     # error details, one file per day
    memory/
      INIT-MEMORY.md               # workspace notes, written by hand
      <id>/                        # one folder per project
        INIT-MEMORY.md             # project notes, written by hand
        annotations.yml
    mcp/
      config.yml                   # MCP server configuration
      contexts/
        production.yml
        staging.yml
        testing.yml
    skills/
```

A `config.yml` looks like this after `init` and one `project add`:

```yaml
version: 1
storage:
  host: localhost
  port: 7687
  namespace: billing-3f9a
projects:
  - id: billing
    path: services/billing
    language: python
    dependency_manager: uv
    environment:
      venv: .venv
      activate: uv run
```

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo doc --no-deps
```

The code is organised as follows:

```
src/
  main.rs                    # entry point and error reporting
  errors.rs                  # top-level error type
  graph/                     # language-neutral code graph
  cli/
    mod.rs                   # argument definitions (clap)
    constants.rs             # file names, templates and detection tables
    errors.rs                # CLI error messages
    table.rs                 # coloured ASCII tables
    prompt.rs                # output and yes/no confirmation
    log.rs                   # daily error logs
    workspace.rs             # finds the workspace and edits its config
    commands/
      init/                  # `workbench init`
      index/                 # `workbench index`: discovery, parsers, storage, lock
        parser/python/       # tree-sitter Python parser
      project/               # `workbench project`
        kinds.rs             # typed languages, dependency managers, frameworks
        add/                 # `workbench project add`
        list/                # `workbench project list`
        remove/              # `workbench project remove`
        cleanup/             # `workbench project cleanup`
        memory.rs            # finding memory folders and their files
templates/                   # files written by `init`
```
