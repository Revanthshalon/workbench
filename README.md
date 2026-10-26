# Workbench

An MCP server (and companion CLI) that turns your services, logs, config and runbooks into tools an agent can use to operate and change a system.

> **Status:** early scaffold. The binaries build but the tool surface below is planned, not implemented.

## Layout

| Path | Crate / binary | Purpose |
|------|----------------|---------|
| [`lib/engine`](lib/engine/README.md) | `engine` | Core: domain, ports, use cases |
| [`services/mcp`](services/mcp/README.md) | `workbench-mcp` | The MCP server |
| [`services/cli`](services/cli/README.md) | `workbench-cli` | Command-line companion (`init`) |
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

## Architecture

Hexagonal (ports and adapters):

```
 clap / MCP  ──►  services (driving adapters)  ──►  lib/engine  ◄──  adapters (driven)
                                                    domain + ports   filesystem, …
```

- `lib/engine` is the core: domain types, port traits and use cases. It does no I/O.
- `services/*` parse their own input (clap, MCP) and call the engine.
- Driven adapters (filesystem, later k8s, databases, logs) live in the service that uses them and implement the engine's ports.

## Development

Requires Rust 1.99.1+ (edition 2024; see `rust-toolchain.toml`).

```sh
cargo build --workspace
cargo clippy --workspace
cargo run -p workbench-cli -- init --out ./tmp   # creates ./tmp/.workbench-cli/config.yml
```
