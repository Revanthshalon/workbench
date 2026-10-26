# engine

The core of Workbench: domain types, ports and use cases. It does no I/O, and has no CLI or filesystem code. Both [`workbench-cli`](../../services/cli/README.md) and [`workbench-mcp`](../../services/mcp/README.md) depend on it.

| Module | Contents |
|--------|----------|
| `domain` | `Platform` |
| `ports` | `ConfigStore`: where the config lives (driven port) |
| `init` | `init()`: creates the config via a `ConfigStore` and resolves the platforms (deduped, defaults if none) |
| `error` | `Error` |

Adapters in the services implement the ports. See [Architecture](../../README.md#architecture).
