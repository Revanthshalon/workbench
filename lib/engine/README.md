# engine

The core of Workbench: domain types, ports and use cases. It does no I/O, and has no CLI or filesystem code. The [`workbench` binary](../../services/cli/README.md), which is both the CLI and the MCP server, depends on it. Static templates the use cases fill in, such as `templates/incident-report.md`, live here.

| Module | Contents |
|--------|----------|
| `domain` | `Platform` |
| `ports` | `ConfigStore`: where the config lives (driven port) |
| `init` | `init()`: creates the config via a `ConfigStore` and resolves the platforms (deduped, defaults if none) |
| `error` | `Error` |

Adapters in the services implement the ports. See [Architecture](../../README.md#architecture).
