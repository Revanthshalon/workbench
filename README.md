# Workbench

An MCP server (and companion CLI) that turns your services, logs, config and runbooks into tools an agent can use to operate and change a system.

> **Status:** early scaffold. The binaries build but the tool surface below is planned, not implemented.

## Layout

| Path | Crate / binary | Purpose |
|------|----------------|---------|
| [`lib/engine`](lib/engine/README.md) | `engine` | Shared core used by both services |
| [`services/mcp`](services/mcp/README.md) | `workbench-mcp` | The MCP server |
| [`services/cli`](services/cli/README.md) | `workbench-cli` | Command-line companion |
| [`docs`](docs/README.md) | | ADRs, HLD, LLD, specs |

## Modes

`workbench-mcp --mode <mode>` picks one bundle of tools, prompts and instructions per server process. To use several modes, register the server several times.

| Mode | For | Status |
|------|-----|--------|
| `operational-support` | Incidents: logs → code flow → root cause → fix | MVP (default) |
| `developer-support` | Context for code changes from the service graph, combined with your LSP setup, context files, prompts and hooks | planned |
| `operational-onboarding` | Do's and don'ts for operating the system, from the graph and your runbooks; no production access | planned |
| `developer-onboarding` | A fast guided tour of the codebase | planned |

See [`services/mcp/README.md`](services/mcp/README.md) for the tool and prompt surface.

## Development

Requires Rust 1.99.1+ (edition 2024; see `rust-toolchain.toml`).

```sh
cargo build --workspace
cargo clippy --workspace
cargo run -p mcp
```
