# Workbench-cli

Command-line companion to [`workbench-mcp`](../mcp/README.md). Binary: `workbench`.

## Usage

```sh
workbench init [--platform <PLATFORM>[,<PLATFORM>...]] [--out <DIR>]
```

Creates `<DIR>/.workbench/config.yml`, empty for now. It refuses to overwrite an existing file.

| Flag | Values | Default |
|------|--------|---------|
| `--platform` | `copilot-cli`, `claude-code`, `codex`; comma-separated or repeated | `copilot-cli,claude-code` |
| `--out` | A directory | `.` (current directory) |

```sh
workbench init --platform copilot-cli,claude-code --out ~/work/my-service
```

## Structure

| Path | Role |
|------|------|
| `src/main.rs` | Wiring only: parse, build the adapter, call `engine`, print |
| `src/cli.rs` | Driving adapter: clap types, mapping to `engine::Platform` |
| `src/adapters/` | Driven adapters implementing `engine` ports (`DirFileStore` implements `ConfigStore`) |
| `src/utils/` | Small helpers (the banner) |
