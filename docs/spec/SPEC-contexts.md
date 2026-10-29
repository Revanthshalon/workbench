# Spec: contexts

Module `contexts` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`core`](SPEC-core.md) (approved). Used by `operations` and `data` (they receive resolved connection settings from here), by [`cli`](SPEC-cli.md) (`workbench context check`) and by `audit`.

## Decisions already made elsewhere

These are fixed by the approved specs and the map; this spec builds on them.

1. Contexts are `production`, `staging` and `testing`. **All are read-only.** `production` has no special rules.
2. Each context has its **own config file**, listed by path in the server config (`.workbench/mcp/config.yml`). This spec owns the content of that file.
3. A context configures connections to Kubernetes (`kubectl`), PostgreSQL, Redis and Kafka, and a code branch.
4. Secrets are written inline or named by `<field>_env` (the variable wins when set), held as `secrecy` values and never printed, logged, serialised or returned by a tool.
5. The **active context is per session**. A new session starts in the server's start context (`workbench mcp start --context C`, default `default_context`). A switch happens only on the user's explicit instruction, is confirmed through MCP elicitation, runs the connection check for the target first, and is refused if the check fails or the client cannot elicit.
6. Connection checks exist for operational modes only and only for the context in use. `workbench context check` never verifies that credentials are read-only. There is no monitoring while the server runs.
7. `developer-support` registers no context tools and never reads any context file.
8. *(Confirmed)* The context names are exactly `production`, `staging` and `testing`.
9. *(Confirmed)* PostgreSQL and Redis can have several **named connections** per context; Kubernetes and Kafka are one cluster per context.
10. *(Confirmed)* Secrets are resolved only for contexts in use: all context files are parsed and validated at start-up, but a context's secret references are resolved for the start context at start and for another context only when a session switches to it. This narrows SPEC-core (which said every context's secrets are resolved at start); SPEC-core is amended to match.
11. *(Confirmed)* A context's **branch is the code that operational analysis runs on**: it is a branch cherry-picked for the release, and in `operational-support` the graph, the code reads and the LSP all see that branch, not whatever the project has checked out. How that is done is described under "The code of a context".
12. *(Confirmed)* The context's branch is checked out as a separate git worktree per project, not by a plain `git checkout` in the project. `workbench context sync` uses the local copy of the branch and contacts a remote only with `--fetch`. The worktree metadata git writes under the project's `.git/worktrees/` is the only change to a project's repository, and that is acceptable.

## Assumptions (correct me now)

1. `kubectl` means the `kubectl` program on the `PATH`, run directly (never through a shell).
2. Every probe has a 10 second timeout (configurable per connection), and `context check` runs the probes of different systems at the same time.
3. Every project used by an operational context is a git repository, and the context's branch exists in it (locally, or as `origin/<branch>`). A project that is not a git repository cannot be used in an operational context, and `context check` says so.

## Objective

Let a person describe where each environment is and how to reach it, in files they can keep out of version control, and let the server use exactly one environment at a time per session, with no chance of silently talking to the wrong one.

The module is done when:
- a context file for each of the three environments validates, and a template with placeholders does not,
- `workbench context check` reports each configured system as `ok`, `failed` or `skipped` without ever printing a secret,
- a session has its own active context, starts in the start context, and can switch only through a confirmed, checked switch,
- in an operational context every analysis runs on that context's branch, without touching the projects' own working trees,
- every operational tool result says which context answered.

### The context file

```yaml
# .workbench/mcp/contexts/staging.yml
version: 1
branch: release-2.4                 # the release branch deployed to this environment
branches:                           # optional per-project overrides
  billing-api: release-2.4.1

kubectl:
  kubeconfig: ~/.kube/config        # optional; default: kubectl's own default
  context: staging-cluster          # the kubectl context name
  namespace: billing                # required
  token_env: STAGING_KUBE_TOKEN     # optional secret; or `token: ...`

postgres:                           # named connections
  billing:
    host: db.staging.internal
    port: 5432                      # default 5432
    database: billing
    user: readonly
    password_env: STAGING_PG_PASSWORD
    sslmode: require                # disable | prefer | require | verify-ca | verify-full; default require
  users:
    host: db.staging.internal
    database: users
    user: readonly
    password_env: STAGING_PG_PASSWORD

redis:                              # named connections
  cache:
    host: cache.staging.internal
    port: 6379                      # default 6379
    db: 0
    tls: false
    password_env: STAGING_REDIS_PASSWORD

kafka:
  brokers: [kafka-1.staging.internal:9092, kafka-2.staging.internal:9092]
  security: sasl_ssl                # plaintext | ssl | sasl_plaintext | sasl_ssl; default plaintext
  sasl_mechanism: SCRAM-SHA-512     # plain | SCRAM-SHA-256 | SCRAM-SHA-512
  username: readonly
  password_env: STAGING_KAFKA_PASSWORD
```

| Rule | Detail |
|---|---|
| `version` | Required, `1`. |
| Sections | Every section (`kubectl`, `postgres`, `redis`, `kafka`) is optional. A system that is not configured is `skipped` and its tools say it is not configured for this context. |
| Unknown keys | Rejected with the file, the path and the key, so a typo cannot silently disable a setting. |
| Placeholders | A value that is still `<fill in>` fails validation and is reported with the others. |
| Secrets | `password`, `token` and the like follow SPEC-core's secret references. A secret field may have an inline value, a `_env` name, or both. |
| `branch` | Required. The branch cherry-picked for the release that runs in this environment. `branches` maps a project id to a different branch for that project (projects in separate repos rarely share branch names). Unknown project ids are rejected. |
| `kubectl.namespace` | Required when `kubectl` is present. |
| Names | The names under `postgres` and `redis` are kebab-case and unique within the context. A context itself is one of `production`, `staging`, `testing`. |

### Server config entry

```yaml
# .workbench/mcp/config.yml (shape owned by SPEC-core)
default_context: staging
contexts:
  production: contexts/production.yml
  staging:    contexts/staging.yml
  testing:    contexts/testing.yml
```

`default_context` must name an entry. Two entries may not point at the same file.

### Active context and sessions

- The engine keeps a map from session id to the session's active context. It is in memory only.
- A new session starts in the server's start context. After a server restart a session starts there again.
- Tools never read a context directly. They ask for the **resolved context** of their session and get its settings, with secrets still wrapped as `Secret`; a connection is opened only inside the adapter that needs it.
- **Switching** (tool `context_switch`):
  1. The agent names the target. The call alone does nothing.
  2. The server asks the client to confirm through elicitation, showing the current context, the target, its branch and which systems it configures. If the client cannot elicit, the switch is refused and the message tells the user to restart the server with `--context`.
  3. On confirmation it resolves the target's secrets and runs the context check for it, including the code check. If anything fails, the switch is refused and the failures are listed (for example `run 'workbench context sync --context production'`).
  4. Only then does the session's active context change, and with it the code the session analyses (the new context's branch). The switch, its check result and the outcome are written to the session log (the audit record).
  5. A refused or declined switch leaves the active context unchanged.
- Every operational tool result includes `context: "<name>"` in its structured output, so both the agent and the person see which environment answered.

### The context check

`workbench context check [--context NAME]` (command defined in `SPEC-cli.md`) runs one harmless probe per configured system, all at once, each with its own timeout:

| System | Probe | `ok` message |
|---|---|---|
| `kubectl` | `kubectl --kubeconfig … --context … get pods -n <namespace> -o name --chunk-size=1 --request-timeout=<t>`: the cluster answers and the namespace is readable | `cluster reachable, namespace '<ns>' readable` |
| `postgres` (each named connection) | Connect with the configured `sslmode`, run `SELECT 1` | `connected as <user>` |
| `redis` (each named connection) | Connect (TLS if set), authenticate if a password is set, `PING` | `PONG` |
| `kafka` | Fetch cluster metadata | `<n> brokers` |
| `code` (all the context's projects) | For each project: the branch exists, a worktree for it exists at the branch's current commit, and its index is current | `3 projects on release-2.4 (billing-api 3f9a1c2, users 81be0d4, web 5c0e7aa), indexed` |

- Results are `ok`, `failed` or `skipped` (not configured), with the latency and a short message. Exit code `0` if nothing failed, `1` otherwise.
- Probes are read-only by construction (`SELECT 1`, `PING`, a metadata request, `get pods`). They never create, write or change anything.
- The `code` line **fails** with the exact fix when a project is not a git repository, the branch does not exist, the worktree is missing, the branch has moved on since the last sync, or the index is behind, for example `billing-api: no worktree for 'release-2.4'. Run 'workbench context sync --context staging'`.
- **No secret in any output.** Messages are built from our own fields. Any text coming from a driver or from `kubectl` has every resolved secret value removed by exact match before it is shown or logged, as a safety net on top of never exposing secrets.

### The code of a context

An operational session must analyse the branch that is released to its environment, not whatever a person happens to have checked out. To do that without touching anyone's working tree:

- **A worktree per context and project.** Workbench keeps a separate checkout of the context's branch for each project at `.workbench/worktrees/<context>/<project-id>/`, created with `git worktree add --detach` at the branch's commit. It is used read-only. The project's own working tree, its current branch, its index and its branch refs are never changed. The one footprint in the project's repository is the worktree metadata git keeps under `.git/worktrees/`.
- **A view per checkout.** The graph keeps one view for the project's working tree (used by `developer-support`) and one for each context (`context:<name>`). Node ids carry the view, so the same file on two branches gives two sets of nodes, and the incremental index works per view by content hash.
- **Sessions use their context's view.** In `operational-support`, `find_symbol`, `code_context`, `trace_flow`, `read_file`, the LSP tools and the rest see the active context's view. Switching context switches the view. `recent_changes` reads the history of the context's branch.
- **Nothing happens implicitly.** A worktree is created or moved only by `workbench context sync`, never by `start` or by a switch, which instead fail with the command to run (the `code` check above).

`workbench context sync [--context NAME] [--project ID]... [--fetch]` (command surface in `SPEC-cli.md`):
- `NAME` defaults to `default_context`; projects default to all of the context's projects.
- For each project it resolves the branch (the override, else the default), finds the branch locally or as `origin/<branch>`, creates the worktree or moves it to the branch's current commit, and runs the incremental index for the view.
- It uses the local copy of the branch. Only with `--fetch` does it first run `git fetch origin <branch>`, which contacts the remote with the person's own git credentials.
- It prints, per project: branch, short commit, files changed since the last sync, and files indexed. It fails naming the project and branch when one cannot be found.
- It only ever manages worktrees it created under `.workbench/worktrees/`.

The language servers for a view run against its worktree, with the project's own environment; how a worktree gets the dependencies it needs (for example a project's `node_modules`) is decided in `SPEC-lsp.md`.

### Tools (candidates; the registry fixes the final names)

| Tool | Kind | Behaviour |
|---|---|---|
| `context_list` | observe | The configured contexts with name, branch and which systems each configures. No secrets. |
| `context_current` | observe | The session's active context and its start context. |
| `context_switch` | record | The confirmed, checked switch above. Changes only the session's in-memory state and writes to the session log. |

These tools exist in `operational-support` only.

### Template

`workbench init` writes one file per default context, built from this fragment. Every connection value is `<fill in>`, so `operational-support` will not start until the start context's file is completed:

```yaml
# .workbench/mcp/contexts/production.yml
version: 1
branch: <fill in>
kubectl:
  context: <fill in>
  namespace: <fill in>
postgres:
  main:
    host: <fill in>
    database: <fill in>
    user: <fill in>
    password_env: <fill in>
# Remove the systems this environment does not use. Add redis: and kafka: sections as needed.
```

## Tech Stack

- Rust 2024, `tokio`, `serde` and `yaml_serde`, `secrecy`, `thiserror` (all already workspace dependencies).
- `kubectl` through `tokio::process::Command` with an argument list, never a shell.
- New dependencies, each needing approval: a PostgreSQL client with TLS (for example `tokio-postgres`), a Redis client (for example `redis`), a Kafka client (for example `rdkafka`, which links the C library librdkafka and needs `cmake`, which the CI job already installs, or the pure-Rust `rskafka`).

## Commands

```
Build:    cargo build --workspace
Test:     cargo test --workspace
Lint:     cargo clippy --workspace --all-targets -- -D warnings
Format:   cargo fmt --all
Sync:     cargo run -p workbench -- context sync --context staging
Check:    cargo run -p workbench -- context check --context staging
Live:     cargo test --workspace -- --ignored    # probes against real services, run by hand
```

## Project Structure

```
lib/engine/src/contexts/
├── name.rs        # Context: the three names (production, staging, testing)
├── config.rs      # ContextConfig and one spec type per system; validation; unknown keys rejected
├── catalog.rs     # all contexts loaded from the server config; default_context validity
├── session.rs     # per-session active context and the switch rules
├── check.rs       # CheckReport, CheckStatus, the check use case
├── code.rs        # branch resolution per project, views, sync and the code check
└── template.rs    # the template fragment

lib/engine/src/ports.rs      # + ContextSource, Probe (one trait, one implementation per system), Confirm (elicitation), Git (branches, worktrees)

lib/adapters/src/contexts/
├── source_file.rs   # reads the server config's context files
├── git_worktree.rs  # the Git port over the `git` program: resolve branch, add or move a worktree
└── probes/{kubectl,postgres,redis,kafka}.rs

services/cli/src/
├── mcp/modules/contexts.rs   # context_list, context_current, context_switch
└── commands/context.rs       # `workbench context check` and `workbench context sync`
```

## Code Style

Each system has its own spec type, so a Redis field cannot end up on a PostgreSQL connection, and unknown keys are an error:

```rust
/// Connection settings for one PostgreSQL database.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresSpec {
    pub host: String,
    #[serde(default = "default_pg_port")]
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(flatten)]
    pub password: SecretRef,        // `password` and/or `password_env`
    #[serde(default)]
    pub sslmode: SslMode,
}
```

## Testing Strategy

- **Engine unit tests:**
  - Validation: required fields, defaults, unknown keys, placeholders, a context name other than the three, bad connection names, duplicate connection names, unknown project ids in `branches`, `default_context` not in the catalog, two entries sharing a file; every problem reported together.
  - Sessions: a new session starts in the start context; two sessions are independent; a switch needs confirmation, runs the target's check first, and leaves the active context unchanged when declined, refused or failed; a client that cannot elicit is refused.
  - The check report: `ok`/`failed`/`skipped`, exit code rule, and the `code` line failing for each case above (not a git repository, branch missing, worktree missing, branch moved on, index behind).
  - Sync: the branch override beats the default; local branch beats `origin/<branch>`; `--fetch` is the only path that contacts a remote; the project's own working tree, branch and refs are untouched; worktrees outside `.workbench/worktrees/` are never touched.
- **Adapter tests** (`lib/adapters`): the `Git` adapter against a temp repository (a release branch, a worktree added, moved after a new commit, a dirty project working tree left exactly as it was); the `kubectl` probe against a fake `kubectl` script placed first on the `PATH` (success, forbidden, timeout, not installed); the file source on valid, invalid and placeholder files; scrubbing removes a resolved secret from a driver error.
- **Live tests** (ignored by default, run by hand against local services): the PostgreSQL, Redis and Kafka probes against real servers, including a wrong password, which must fail without the password appearing in the output.
- **Integration tests** (`services/cli/tests/`): `workbench context check` in a temp workspace with fake probes, covering output format, exit codes and the "server config or context file missing or placeholders" errors.
- Every row of the probe table and every rule in "Active context and sessions" has a test.

## Boundaries

- **Always:** resolve a context's secrets only when it is in use; time out every probe; keep probes read-only; put `context: <name>` in every operational tool result; remove resolved secret values from driver and `kubectl` text before showing or logging it.
- **Ask first:** adding a dependency (PostgreSQL, Redis, Kafka clients, TLS); supporting another kind of system; adding a context name; changing the context file `version`; removing worktrees automatically.
- **Never:** change a project's own working tree, current branch, index or branch refs; touch a worktree Workbench did not create; run `git fetch` unless `--fetch` was given; switch a session's context without confirmation and a passing check; check or contact a context nobody asked for; print, log or return a secret; run a `kubectl` verb other than the probe's from this module; create, write or change anything in an environment from a probe; read a context file in `developer-support`.

## Success Criteria

1. A valid context file for each of `production`, `staging` and `testing` loads; the template with placeholders fails with every placeholder reported.
2. An unknown key, a missing required field, a bad name or a duplicate connection name is rejected with the file and the path.
3. `workbench context check` prints `ok`, `failed` or `skipped` per configured system, including the `code` line, with latency, exits `1` if any failed, and checks only the named context.
4. `workbench context sync` creates or moves one worktree per project at the context's branch, indexes its view, and leaves each project's own working tree, branch and refs exactly as they were; `--fetch` is the only way it contacts a remote.
5. In `operational-support` a session's code reads, graph queries and LSP tools see the active context's branch; after a switch they see the new context's branch.
6. No secret value appears in the output or the logs of a check or a sync, including after a failed login.
7. A new session starts in the start context; two sessions switch independently.
8. `context_switch` changes the session's context only after elicitation confirmation and a passing check, and is refused (leaving the context unchanged) when declined, when the check fails or when the client cannot elicit; each attempt is in the session log.
9. Every operational tool result names the context that answered.
10. `developer-support` registers no context tools and never reads a context file.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- `workbench context clean`: remove a context's worktrees. For now they stay until a person removes them (`git worktree remove`).

## Open Questions

None. The questions from the first draft are answered and folded into the decisions above.
