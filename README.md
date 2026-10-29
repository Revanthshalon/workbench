# Workbench

An MCP server (and companion CLI) that turns your services, logs, config and runbooks into tools an agent can use to operate and change a system.

> **Status:** early scaffold. The binaries build but the tool surface below is planned, not implemented.

## Layout

| Path | Crate / binary | Purpose |
|------|----------------|---------|
| [`lib/engine`](lib/engine/README.md) | `engine` | Core: domain, ports, use cases |
| [`services/cli`](services/cli/README.md) | `workbench` | The one binary: the CLI and the MCP server (`workbench mcp`) |
| [`docs`](docs/README.md) | | ADRs, HLD, LLD, specs |

## Modes

`workbench mcp start --mode <mode>` picks one bundle of tools, prompts and instructions per server; the default is `developer-support`. To use several modes, run one server per mode; AI platforms connect to each with `workbench mcp connect`.

| Mode | For | Status |
|------|-----|--------|
| `operational-support` | Incidents: logs → code flow → root cause → fix | MVP |
| `developer-support` | Context for code changes from the service graph and its semantic annotations (kept in the database), combined with your LSP setup, prompts and hooks | MVP (default) |
| `operational-onboarding` | Do's and don'ts for operating the system, from the graph and your runbooks; no production access | planned |
| `developer-onboarding` | A fast guided tour of the codebase | planned |

See [`services/cli/README.md`](services/cli/README.md) for the commands and the MCP tool and prompt surface.

## Architecture

Hexagonal (ports and adapters):

```
 clap / MCP  ──►  services (driving adapters)  ──►  lib/engine  ◄──  adapters (driven)
                                                    domain + ports   filesystem, …
```

- `lib/engine` is the core: domain types, port traits and use cases. It does no I/O.
- `services/*` parse their own input (clap, MCP) and call the engine.
- Driven adapters (filesystem, later k8s, databases, logs) live in the service that uses them and implement the engine's ports.

## Storage

Workbench keeps the code graph and its semantic annotations in **Memgraph** (the default) or **SQLite**. Memgraph runs on your own machine; Workbench does not install or start it. For example:

```sh
docker run -p 7687:7687 memgraph/memgraph
```

To use SQLite instead, which needs no server, set `storage.backend: sqlite` in `.workbench/config.yml`. `workbench storage status` shows which backend is in use and whether it answers.

## Development

Requires Rust 1.99.1+ (edition 2024; see `rust-toolchain.toml`).

```sh
cargo build --workspace
cargo clippy --workspace
cargo run -p workbench -- init --out ./tmp   # creates ./tmp/.workbench/config.yml
```
