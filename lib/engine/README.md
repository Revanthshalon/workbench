# engine

The core of Workbench: domain types, ports and use cases. It does no I/O, and has no CLI or filesystem code. The [`workbench` binary](../../services/cli/README.md), which is both the CLI and the MCP server, depends on it. Static templates the use cases fill in, such as `templates/incident-report.md`, live here.

| Module | Contents |
|--------|----------|
| `workspace` | The workspace model (`ProjectId`, `Language`, `Framework`, `DependencyManager`, `Project`, `Binary`, `WorkspaceConfig`), validation, LSP precedence, project scope, memory templates and the `add_project`, `list_projects` and `remove_project` use cases |
| `ports` | Driven ports: `WorkspaceStore`, `MemoryStore`, `ProjectContextSource`, `ProjectInspector`, `GitignoreStore`, `IdSource` |
| `init` | `init_workspace()`: writes the empty config, workspace memory and the `.gitignore` entry |
| `error` | `Error` |
| `testing` | Recording fakes for the ports, compiled for tests only |

The engine has no `serde`: parsing and file formats belong to the adapters ([ADR-0001](../../docs/adr/0001-engine-free-of-serde.md)). Design: [`LLD-workspace-cli.md`](../../docs/lld/LLD-workspace-cli.md). The module list above is the Slice A target ([plan](../../tasks/plan.md)); until it is built, the scaffold has `Platform`, `ConfigStore` and `init()`.

Adapters in the services implement the ports. See [Architecture](../../README.md#architecture).
