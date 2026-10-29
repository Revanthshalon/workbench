# Spec: contexts

Module `contexts` from the [capability map](capability-map.md). Status: **approved.**

Depends on [`core`](SPEC-core.md) (approved). Used by `operations` and `data` (they receive resolved connection settings from here), by [`cli`](SPEC-cli.md) (`workbench context check`) and by `audit`.

## Decisions already made elsewhere

These are fixed by the approved specs and the map; this spec builds on them.

1. Contexts are `production`, `staging` and `testing`. **All are read-only.** `production` has no special rules.
2. Each context has its **own config file**, listed by path in the server config (`.workbench/mcp/config.yml`). This spec owns the content of that file.
3. A context configures how to reach its VM over SSH, and from there Kubernetes (`kubectl`), PostgreSQL, Redis and Kafka, plus a code branch.
4. Secrets are written inline or named by `<field>_env` (the variable wins when set), held as `secrecy` values and never printed, logged, serialised or returned by a tool.
5. The **active context is per session**. A new session starts in the server's start context (`workbench mcp start --context C`, default `default_context`). A switch happens only on the user's explicit instruction, is confirmed through MCP elicitation, runs the connection check for the target first, and is refused if the check fails or the client cannot elicit.
6. Connection checks exist for operational modes only and only for the context in use. `workbench context check` never verifies that credentials are read-only. There is no monitoring while the server runs.
7. `developer-support` registers no context tools and never reads any context file.
8. *(Confirmed)* The context names are exactly `production`, `staging` and `testing`.
9. *(Confirmed)* PostgreSQL and Redis can have several **named connections** per context; Kubernetes and Kafka are one cluster per context.
10. *(Confirmed)* Secrets are resolved only for contexts in use: all context files are parsed and validated at start-up, but a context's secret references are resolved for the start context at start and for another context only when a session switches to it. This narrows SPEC-core (which said every context's secrets are resolved at start); SPEC-core is amended to match.
11. *(Confirmed)* A context's **branch is the code that operational analysis runs on**: it is a branch cherry-picked for the release, and in `operational-support` the graph, the code reads and the LSP all see that branch, not whatever the project has checked out. How that is done is described under "The code of a context".
12. *(Confirmed)* The context's branch is checked out as a separate git worktree per project, not by a plain `git checkout` in the project. `workbench context sync` uses the local copy of the branch and contacts a remote only with `--fetch`. The worktree metadata git writes under the project's `.git/worktrees/` is the only change to a project's repository, and that is acceptable.
13. *(Confirmed)* **Each context has its own VM and its own credentials.** Workbench logs in to the VM over SSH and runs the commands the mode needs there (`kubectl` and whatever else the operational tools require). `kubectl` is not run on the workstation, and the cluster's own credentials stay on the VM.

## Assumptions (correct me now)

1. SSH authentication is a **password** (a secret, inline or named by `password_env`). There is no key-file or ssh-agent login in the first release and no agent forwarding. The VM's host key is always verified: it must already be in `known_hosts` or be pinned in the context file, and an unknown or changed key is refused, never accepted automatically.
2. Operational tools never send a free-form shell string. Each remote operation is a **typed command template** (a fixed program and argument list with validated parameters), defined by `operations`, and run through one **remote runner** that allows nothing else: no shell, no TTY, no `sudo`, no redirection, with a time limit (30 seconds by default) and an output limit (5 MB by default).
3. *(Confirmed: configurable)* PostgreSQL, Redis and Kafka can be reached in either of two ways, chosen per connection with `access`:
   - `tunnel` (the default): a port forward from `127.0.0.1` on this machine to the host and port as the VM sees them, used by the native drivers locally. Read-only enforcement lives in our own code (`data` spec), and the datastore credentials come from the context file.
   - `remote`: the VM's own `psql`, `redis-cli` or Kafka tools run through the remote runner as typed command templates (`data` spec). The tools must be installed on the VM, and their text output is parsed. Datastore passwords are **never** sent to the VM: in this mode the login is whatever the SSH user already has there (for example a `.pgpass` file), so a `remote` connection may not carry a password in the context file.
4. One SSH connection per context, opened on first use, shared by every session in that context, closed after 10 idle minutes, and re-opened once automatically if it drops.
5. Every probe has a 10 second timeout (configurable per connection), and `context check` runs independent probes at the same time.
6. Every project used by an operational context is a git repository, and the context's branch exists in it (locally, or as `origin/<branch>`). A project that is not a git repository cannot be used in an operational context, and `context check` says so.

## Objective

Let a person describe where each environment is, how to log in to its VM and what runs behind it, in files they can keep out of version control, and let the server use exactly one environment at a time per session, with no chance of silently talking to the wrong one.

The module is done when:
- a context file for each of the three environments validates, and a template with placeholders does not,
- `workbench context check` reports the SSH login and each configured system as `ok`, `failed` or `skipped` without ever printing a secret,
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

ssh:                                # required: how to log in to this environment's VM
  host: vm-staging.internal
  port: 22                          # default 22
  user: workbench
  password_env: STAGING_SSH_PASSWORD       # the SSH password; or `password: ...` inline
  host_key: SHA256:abc123...        # optional pin; otherwise known_hosts must already list the host
  connect_timeout: 10               # seconds

kubectl:                            # runs ON the VM, through ssh
  binary: kubectl                   # default
  kubeconfig: /home/workbench/.kube/config   # path on the VM; optional
  context: staging-cluster          # the kubectl context name
  namespace: billing                # required

postgres:                           # named connections; host and port as the VM sees them
  billing:
    host: db.staging.internal
    port: 5432                      # default 5432
    database: billing
    user: readonly
    password_env: STAGING_PG_PASSWORD
    sslmode: require                # disable | prefer | require | verify-ca | verify-full; default require
    access: tunnel                  # tunnel (default) | remote
  users:
    host: db.staging.internal
    database: users
    user: readonly
    access: remote                  # run the VM's psql; no password here, the VM's .pgpass is used
    binary: psql                    # remote only; default psql

redis:                              # named connections, reached through the VM
  cache:
    host: cache.staging.internal
    port: 6379                      # default 6379
    db: 0
    tls: false
    password_env: STAGING_REDIS_PASSWORD
    access: tunnel                  # tunnel (default) | remote (then `binary: redis-cli`, no password here)

kafka:                              # reached through the VM
  brokers: [kafka-1.staging.internal:9092, kafka-2.staging.internal:9092]
  security: sasl_ssl                # plaintext | ssl | sasl_plaintext | sasl_ssl; default plaintext
  sasl_mechanism: SCRAM-SHA-512     # plain | SCRAM-SHA-256 | SCRAM-SHA-512
  username: readonly
  password_env: STAGING_KAFKA_PASSWORD
  access: tunnel                    # tunnel (default) | remote (then `bin_dir:` on the VM, no password here)
```

| Rule | Detail |
|---|---|
| `version` | Required, `1`. |
| `ssh` | Required: every operational system is reached through the VM. `host`, `user` and a password secret (`password` or `password_env`) are required. `host_key` is an optional pin (`SHA256:…`). |
| `access` | Optional on each `postgres` and `redis` connection and on `kafka`; `tunnel` (default) or `remote`. With `remote`, a password or other secret field on that connection is rejected ("passwords are not sent to the VM; remove it or use `access: tunnel`"), and `binary` (`psql`, `redis-cli`) or `bin_dir` (Kafka tools) name the program on the VM. With `tunnel`, `binary` and `bin_dir` are rejected. |
| Sections | Every other section (`kubectl`, `postgres`, `redis`, `kafka`) is optional. A system that is not configured is `skipped` and its tools say it is not configured for this context. |
| Unknown keys | Rejected with the file, the path and the key, so a typo cannot silently disable a setting. |
| Placeholders | A value that is still `<fill in>` fails validation and is reported with the others. |
| Secrets | `password` and the like follow SPEC-core's secret references. A secret field may have an inline value, a `_env` name, or both. |
| `branch` | Required. The branch cherry-picked for the release that runs in this environment. `branches` maps a project id to a different branch for that project (projects in separate repos rarely share branch names). Unknown project ids are rejected. |
| `kubectl.namespace` | Required when `kubectl` is present. `kubectl.kubeconfig` is a path on the VM. The cluster's credentials live on the VM, not in this file. |
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

### Reaching the environment: SSH

All runtime access to an environment goes through its VM.

- **The connection.** One SSH connection per context, opened the first time something in that context needs it, shared by the sessions in that context, closed after 10 idle minutes and re-opened once if it drops. If it cannot be opened the tool fails with an error naming the context and the host.
- **Host keys.** The host key must already be in `known_hosts` or match the pin in the context file. An unknown key, a changed key or a pin mismatch is refused with the fingerprint shown, so a person can verify and add it. Keys are never accepted automatically.
- **No shell, no agent.** No interactive shell, no TTY, no agent forwarding, no `sudo`. The credential is the SSH password from the context file (a `secrecy` value, exposed only to authenticate).
- **The remote runner.** The only way to run something on the VM. It accepts a typed command template from `operations` (a fixed program and arguments, with each parameter validated, for example a namespace or a pod name against a strict pattern) and nothing else. The catalog of templates (`kubectl get`, `describe`, `logs`, `docker compose ps`, reading a configuration file, and so on) is owned by `operations`; the runner refuses a command that is not in it. Each run has a time limit (30 seconds by default) and an output limit (5 MB by default), and returns the exit code and the output.
- **Two ways to reach a datastore** (`access`, per connection). **Tunnel:** the runner opens a port forward bound to `127.0.0.1` only, to the host and port in the context file as the VM sees them, and the native driver connects to the local end; tunnels close with the connection. **Remote:** the VM's own `psql`, `redis-cli` or Kafka tools run through the runner as typed templates and their output is parsed; no datastore password ever leaves this machine, because the tools use the SSH user's own login on the VM.
- **Audit.** Each remote command (program, arguments, exit code, output size) and each tunnel opening is a line in the session log, which is the audit record. Output itself is subject to the rules in `SPEC-audit.md`. Credentials never appear.

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
| `ssh` | Open the connection, verify the host key, authenticate, run `true` on the VM | `logged in as <user> on <host> (host key SHA256:…)` |
| `kubectl` | On the VM: `kubectl [--kubeconfig …] [--context …] get pods -n <namespace> -o name --chunk-size=1 --request-timeout=<t>`: the cluster answers and the namespace is readable | `cluster reachable, namespace '<ns>' readable` |
| `postgres` (each named connection) | Tunnel: connect with the configured `sslmode`, run `SELECT 1`. Remote: on the VM, `psql -w -h … -U … -d … -tAc 'SELECT 1'` | `connected as <user> (tunnel)` or `(on the VM)` |
| `redis` (each named connection) | Tunnel: connect (TLS if set), authenticate, `PING`. Remote: on the VM, `redis-cli -h … -p … PING` | `PONG (tunnel)` or `(on the VM)` |
| `kafka` | Tunnel: fetch cluster metadata. Remote: on the VM, the Kafka tools' broker API check | `<n> brokers (tunnel)` or `(on the VM)` |
| `code` (all the context's projects) | For each project: the branch exists, a worktree for it exists at the branch's current commit, and its index is current | `3 projects on release-2.4 (billing-api 3f9a1c2, users 81be0d4, web 5c0e7aa), indexed` |

- Results are `ok`, `failed` or `skipped` (not configured), with the latency and a short message. Exit code `0` if nothing failed, `1` otherwise.
- If the `ssh` probe fails, the probes that depend on it (`kubectl`, `postgres`, `redis`, `kafka`) are not attempted and are shown as `skipped` with the message `ssh failed`; the `code` line does not need the VM and still runs.
- Probes are read-only by construction (`true`, `SELECT 1`, `PING`, a metadata request, `get pods`). They never create, write or change anything.
- The `code` line **fails** with the exact fix when a project is not a git repository, the branch does not exist, the worktree is missing, the branch has moved on since the last sync, or the index is behind, for example `billing-api: no worktree for 'release-2.4'. Run 'workbench context sync --context staging'`.
- **No secret in any output.** Messages are built from our own fields. Any text coming from a driver, from the SSH library or from a command run on the VM has every resolved secret value removed by exact match before it is shown or logged, as a safety net on top of never exposing secrets.

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
| `context_list` | observe | The configured contexts with name, branch, the SSH host and user, and which systems each configures. No secrets. |
| `context_current` | observe | The session's active context and its start context. |
| `context_switch` | record | The confirmed, checked switch above. Changes only the session's in-memory state and writes to the session log. |

These tools exist in `operational-support` only.

### Template

`workbench init` writes one file per default context, built from this fragment. Every connection value is `<fill in>`, so `operational-support` will not start until the start context's file is completed:

```yaml
# .workbench/mcp/contexts/production.yml
version: 1
branch: <fill in>
ssh:
  host: <fill in>
  user: <fill in>
  password_env: <fill in>
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
- SSH: an in-process client library (for example `russh`) for the connection, command channels and port forwarding. The alternative is driving the `ssh` program, which brings `~/.ssh/config` support for free but needs process management and gives less control over tunnels.
- `git` through `tokio::process::Command` with an argument list, never a shell.
- New dependencies, each needing approval: the SSH library, a PostgreSQL client with TLS (for example `tokio-postgres`), a Redis client (for example `redis`), a Kafka client (for example `rdkafka`, which links the C library librdkafka and needs `cmake`, which the CI job already installs, or the pure-Rust `rskafka`).

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
├── ssh.rs         # SshSpec (host, auth, host key pin) and validation
├── remote.rs      # CommandTemplate (fixed program, validated parameters) and the allow rule the runner enforces
└── template.rs    # the template fragment

lib/engine/src/ports.rs      # + ContextSource, Probe (one trait, one implementation per system), Confirm (elicitation), Git (branches, worktrees), Remote (run a typed command on the VM, open a tunnel)

lib/adapters/src/contexts/
├── source_file.rs   # reads the server config's context files
├── git_worktree.rs  # the Git port over the `git` program: resolve branch, add or move a worktree
├── ssh_remote.rs    # the Remote port over SSH: connection per context, host key check, runner, tunnels
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
  - Validation: the `ssh` section (host and user required, exactly one of key or password, a bad pin), required fields, defaults, unknown keys, placeholders, a context name other than the three, bad connection names, duplicate connection names, unknown project ids in `branches`, `default_context` not in the catalog, two entries sharing a file; every problem reported together.
  - `access`: a `remote` connection with a password field is rejected; `binary`/`bin_dir` are rejected with `tunnel`; the default is `tunnel`; both modes yield the same check result for the same fake datastore.
  - The remote runner: a command that is not a typed template is refused; a parameter that does not match its pattern is refused; no shell, TTY or `sudo` is ever requested; the time and output limits apply.
  - Sessions: a new session starts in the start context; two sessions are independent; a switch needs confirmation, runs the target's check first, and leaves the active context unchanged when declined, refused or failed; a client that cannot elicit is refused.
  - The check report: `ok`/`failed`/`skipped`, exit code rule, and the `code` line failing for each case above (not a git repository, branch missing, worktree missing, branch moved on, index behind).
  - Sync: the branch override beats the default; local branch beats `origin/<branch>`; `--fetch` is the only path that contacts a remote; the project's own working tree, branch and refs are untouched; worktrees outside `.workbench/worktrees/` are never touched.
- **Adapter tests** (`lib/adapters`): the `Git` adapter against a temp repository (a release branch, a worktree added, moved after a new commit, a dirty project working tree left exactly as it was); the `kubectl` probe through a fake `Remote` (success, forbidden, timeout, not installed); the SSH adapter against a test SSH server (password login, a wrong password refused, an unknown host key refused, a changed key refused, a pin mismatch refused, tunnels bound to `127.0.0.1` only, a dropped connection re-opened once, idle close); the file source on valid, invalid and placeholder files; scrubbing removes a resolved secret from a driver error.
- **Live tests** (ignored by default, run by hand against a real VM or local services): the SSH login, and the PostgreSQL, Redis and Kafka probes against real servers in both `tunnel` and `remote` mode, including a wrong password, which must fail without the password appearing in the output.
- **Integration tests** (`services/cli/tests/`): `workbench context check` in a temp workspace with fake probes, covering output format, exit codes and the "server config or context file missing or placeholders" errors.
- Every row of the probe table and every rule in "Active context and sessions" has a test.

## Boundaries

- **Always:** resolve a context's secrets only when it is in use; verify the VM's host key; bind tunnels to `127.0.0.1`; run remote work only through the remote runner; time out every probe; keep probes read-only; put `context: <name>` in every operational tool result; remove resolved secret values from driver and `kubectl` text before showing or logging it.
- **Ask first:** adding a dependency (the SSH library, PostgreSQL, Redis, Kafka clients, TLS); jump hosts (bastions), key or ssh-agent login, or sending any datastore secret to the VM; supporting another kind of system; adding a context name; changing the context file `version`; removing worktrees automatically.
- **Never:** change a project's own working tree, current branch, index or branch refs; touch a worktree Workbench did not create; run `git fetch` unless `--fetch` was given; switch a session's context without confirmation and a passing check; check or contact a context nobody asked for; print, log or return a secret; run anything on a VM outside the remote runner; accept a host key automatically; forward an agent, open a TTY or a shell, or use `sudo`; bind a tunnel beyond `127.0.0.1`; run a `kubectl` command other than the probe's from this module; create, write or change anything in an environment from a probe; read a context file in `developer-support`.

## Success Criteria

1. A valid context file for each of `production`, `staging` and `testing` loads; the template with placeholders fails with every placeholder reported.
2. An unknown key, a missing required field, a bad name or a duplicate connection name is rejected with the file and the path.
3. `workbench context check` prints `ok`, `failed` or `skipped` for the SSH login and each configured system, including the `code` line (systems behind a failed login are shown as skipped), with latency, exits `1` if any failed, and checks only the named context.
4. `workbench context sync` creates or moves one worktree per project at the context's branch, indexes its view, and leaves each project's own working tree, branch and refs exactly as they were; `--fetch` is the only way it contacts a remote.
5. In `operational-support` a session's code reads, graph queries and LSP tools see the active context's branch; after a switch they see the new context's branch.
6. SSH to the VM verifies the host key (an unknown, changed or mismatched key is refused), uses only the configured password, and runs only typed command templates through the remote runner; tunnels listen on `127.0.0.1` only; each remote command and tunnel appears in the session log.
7. No secret value appears in the output or the logs of a check or a sync, including after a failed login.
8. A new session starts in the start context; two sessions switch independently.
9. `context_switch` changes the session's context only after elicitation confirmation and a passing check, and is refused (leaving the context unchanged) when declined, when the check fails or when the client cannot elicit; each attempt is in the session log.
10. Every operational tool result names the context that answered.
11. `developer-support` registers no context tools and never reads a context file.
12. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Roadmap (not in this spec)

- Jump hosts (bastions): a `jump` setting in the `ssh` section. The first release logs in to the VM directly.
- SSH key and ssh-agent login. The first release uses a password.
- `workbench context clean`: remove a context's worktrees. For now they stay until a person removes them (`git worktree remove`).

## Open Questions

None. The questions from the earlier drafts are answered and folded into the decisions and assumptions above.
