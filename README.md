# Workbench

Workbench is a command-line tool that sets up a **workspace** of projects and
records what the workspace contains. Each workspace keeps its state in a
`.workbench/` directory at its root.

## Requirements

- Rust (the toolchain pinned in `rust-toolchain.toml`)

## Build

```sh
cargo build --release
./target/release/workbench --help
```

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
| `--index` | Accepted, but currently only prints a warning: `workbench index` does not exist yet. |

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

## Workspace layout

```
<workspace>/
  .gitignore                       # contains .workbench/
  .workbench/
    config.yml                     # version, storage and projects
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
  cli/
    mod.rs                   # argument definitions (clap)
    constants.rs             # file names, templates and detection tables
    errors.rs                # CLI error messages
    table.rs                 # coloured ASCII tables
    prompt.rs                # output and yes/no confirmation
    workspace.rs             # finds the workspace and edits its config
    commands/
      init/                  # `workbench init`
      project/               # `workbench project`
        kinds.rs             # typed languages, dependency managers, frameworks
        add/                 # `workbench project add`
        list/                # `workbench project list`
        remove/              # `workbench project remove`
        cleanup/             # `workbench project cleanup`
        memory.rs            # finding memory folders and their files
templates/                   # files written by `init`
```
