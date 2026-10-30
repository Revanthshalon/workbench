# Spec: operations

Module `operations` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`contexts`](SPEC-contexts.md) (approved: the VM, the remote runner, the active context and its branch view), [`audit`](SPEC-audit.md) (draft: every remote command is recorded), [`graph`](SPEC-graph.md) (draft: mapping log frames and config to code) and [`core`](SPEC-core.md) (approved). Used by `execution-flow` (`error_paths`, `request_timeline`) and `flow-memory`. `operational-support` only; `developer-support` never loads it.

## Decisions already made elsewhere

1. **Everything runs on the context's VM through the one remote runner** (`SPEC-contexts.md`): typed command templates only, no shell, no TTY, no `sudo`, 30 seconds and 5 MB per command by default. `kubectl` is not run on the workstation.
2. **This module owns the catalog of command templates** the runner accepts for operations (`SPEC-contexts.md`, Assumption 2). The runner refuses any command that is not in a catalog.
3. **Read-only.** All contexts are read-only and no tool changes an observed system (`SPEC-core.md`). Every tool here is kind `observe`.
4. **The first log source is `kubectl`**, with a configurable namespace: `kubectl.namespace` in the context file, plus `kubeconfig` and `context` there.
5. Tools: `fetch_logs`, `match_log`, `analyze_logs`, `compose_status`, `k8s_status`, `service_config`, `check_config`. Each answer says which context answered (`context: "<name>"`), and code is read from that context's branch view.
6. No pattern-based redaction anywhere. Secrets Workbench holds are removed from output by exact match as a safety net (`SPEC-contexts.md`, `SPEC-audit.md`); credentials never reach the log because they are never exposed.
7. A system that is not configured for the context answers "not configured for this context", never an error from the runner.

## Assumptions (correct me now)

1. **A "service" is a Kubernetes Deployment name** (or, for Compose, a Compose service name) in the context's namespace. Its pods come from the Deployment's own selector (`matchLabels`), read with the `deployments` template, so no label convention has to be configured. A tool may also be given a pod name.
2. **One namespace per context** in the first release (`kubectl.namespace`). A tool never takes a namespace argument (Open Question 2).
3. **Command output is parsed in the engine and only selected fields are returned.** `kubectl get … -o json` is parsed and reduced to the fields a tool names; the raw document is never returned or recorded beyond the audit rules. This is how environment values and ConfigMap values stay out of answers.
4. **Values are never returned for configuration.** `service_config` and `check_config` return names, counts, hashes and non-sensitive structure (image, ports, replicas, resource limits), never an environment variable's value, a ConfigMap value or a Secret. The catalog has no template that reads a Secret (Open Question 4).
5. **Logs are returned as the application wrote them.** Only the secrets Workbench itself holds are removed (Assumption 6 above); a token the application prints is returned. This is stated in the tool descriptions (Open Question 3).
6. **Log time and size limits are bounded:** `since` default 15 minutes, at most 24 hours; `tail` default 500, at most 5 000 lines per pod; at most 5 pods per call (newest first); 2 000 lines or 1 MB returned, whichever is hit first, with a "cut" notice that says how to narrow the call. When the runner's 5 MB output limit is hit the answer says the output was truncated.
7. **Parallelism per context:** at most 4 remote commands at once on the context's SSH connection; further commands wait.
8. **Log analysis is deterministic, no LLM.** `analyze_logs` groups lines by a normalised signature, detects stack traces, and maps frames to code nodes; any interpretation is the agent's.
9. **Docker Compose on the VM needs a context-file section** the approved `SPEC-contexts.md` does not have (the directory and file names on the VM). Proposed as an amendment (Open Question 1); until then `compose_status` answers "not configured".
10. The map lists `lsp` as a dependency of this module. Mapping frames to code uses the graph only, so `lsp` is not used here (Open Question 5).

## Objective

Let an agent see what an environment is doing (what runs, what it logged, how it is configured) fast and safely, so it can reason from evidence, without a person copy-pasting from a terminal and without any way to change what it observes.

The module is done when:
- each tool answers from the active context's VM through typed templates only, with inputs validated and answers bounded,
- log answers are grouped and mapped to code on the context's branch where possible,
- configuration answers never contain a value, and a secret planted in a fixture never appears in any answer,
- every remote command is visible in the session log and the catalog can be listed and tested as data.

## The catalog

Templates are data; each has a fixed program and arguments and typed parameters. Every parameter has a validator; anything else is refused before a connection is used. Every argument is passed as its own element and quoted when the line is built, and none may start with `-`.

| Template | Command on the VM | Parameters |
|---|---|---|
| `k8s.pods` | `kubectl get pods -n NS -o json [-l SELECTOR]` | namespace, optional selector |
| `k8s.deployments` | `kubectl get deployments -n NS -o json` | namespace |
| `k8s.events` | `kubectl get events -n NS --sort-by=.lastTimestamp -o json` | namespace |
| `k8s.configmap` | `kubectl get configmap NAME -n NS -o json` (parsed; values dropped at once) | namespace, name |
| `k8s.logs` | `kubectl logs POD -n NS [-c CONTAINER] [--previous] --timestamps (--since=DUR \| --tail=N)` | namespace, pod, container, duration, count |
| `compose.ps` | `docker compose [-f FILE]… --project-directory DIR ps --format json` | directory, files |
| `compose.logs` | `docker compose … logs --no-color --timestamps (--since DUR \| --tail N) SERVICE` | directory, files, service, duration, count |
| `compose.config` | `docker compose … config --format json` (parsed; values dropped at once) | directory, files |

`kubectl` always gets the context's `--kubeconfig` and `--context` when set and `--request-timeout`. The binary names come from the context file.

| Parameter | Validator |
|---|---|
| namespace, pod, container, deployment, configmap, compose service | DNS-1123 style: lowercase letters, digits, `-`, `.`; at most 63 characters (253 for a pod); must start and end alphanumeric. Compose service names also allow `_`. |
| selector | at most 5 `key=value` or `key!=value` pairs, key `[A-Za-z0-9._/-]+`, value `[A-Za-z0-9._-]+` |
| duration | digits followed by `s`, `m` or `h`, at most 24 hours |
| count | an integer from 1 to 5 000 |
| directory, files | absolute paths on the VM from the context file; never from a tool call |

The catalog contains only `get` and `logs` (kubectl) and `ps`, `logs` and `config` (Compose). There is no `exec`, `cp`, `port-forward`, `apply`, `delete`, `edit`, `patch`, `scale`, `rollout`, `describe`, `top`, `run`, `up`, `down`, `restart` and no read of a Secret. A test enumerates the catalog to keep it that way.

## The tools

| Tool | Input (besides the session's context) | Answer |
|---|---|---|
| `fetch_logs` | `service` or `pod`, `since`, `tail`, `container`, `previous`, `pattern` (case-insensitive contains, applied here), `level` | Lines `{time, pod, container, level, text}` newest last, per-pod counts, the cut and truncation notices |
| `match_log` | `service` or `pod`, `pattern`, `regex` (default false), `before`, `after` (each at most 5), `since`, `max_matches` (default 50) | Matches with their surrounding lines, grouped by pod, with count, first and last time. A regex is at most 200 characters and runs in linear time |
| `analyze_logs` | `service` or `pod`, `since`, `compare_previous` (default true) | **Signatures** (normalised messages) with count, level, first and last seen and whether new compared with the window just before; error counts per minute; stack traces with frames mapped to code nodes; the pods' restarts and states |
| `k8s_status` | `service` (optional) | Pods (phase, ready, restarts, last termination reason and exit code, age), deployments (desired, ready, updated, images), warning events from the last hour (at most 50), and **findings** with the evidence for each |
| `compose_status` | `service` (optional) | Services with state, health, ports, restarts and image; findings for exited, unhealthy and restarting services |
| `service_config` | `service` | Image, ports, replicas, resource requests and limits, probes, environment variable **names** with where each comes from (literal, ConfigMap name, Secret name), ConfigMap keys, mounted names. Never values |
| `check_config` | `service` | Drift between the running service and the code of the context's branch (below) |

A call with an unknown service lists the known ones. A call when `kubectl` is not configured says so. If the SSH connection fails, the tool fails with an error naming the context and host.

### Findings (`k8s_status`, `compose_status`)

A finding is `{kind, subject, evidence}` from fixed rules: `CrashLoopBackOff`, `ImagePullBackOff`, `OOMKilled` (last termination reason), `Pending` with the scheduler's message, `NotReady`, `RolloutStalled` (updated ≠ desired), `HighRestarts` (at least 5 in the pod's age), and for Compose `Exited`, `Unhealthy`, `Restarting`. Every finding names the field it came from; there are no guesses about causes.

### `analyze_logs`

```text
fetch the window (and the same-sized window before it when compare_previous)
  → parse each line: time, level (JSON `level`/`severity`, else ERROR/WARN/INFO/Traceback words)
  → signature = the message with numbers, UUIDs, hex, IDs, timestamps and quoted values replaced by placeholders
  → count by signature; mark "new" when absent from the previous window
  → find Python tracebacks and Node/TypeScript stack traces; each frame is file, line, function
  → map frames: a frame's path ending in a project file path, in the context's view, to the nodes at those lines
  → add pod restarts and states
```

A frame that maps to nothing stays as text. The mapping reads only the context's view, so it points at the code that is deployed, not the working tree.

### `check_config`

Compares the running service with what the context's branch says it should have, using the graph's Compose and Dockerfile nodes in that view:

| Check | Source on each side | Reported as |
|---|---|---|
| Environment names | the workload's variable names against the Compose service's `environment_keys` and the Dockerfile's `ENV` names | `missing_in_runtime`, `unexpected_in_runtime` (information, not an error) |
| Ports | the container ports against `EXPOSE` and the Compose `ports` | `port_mismatch` |
| Image | the image tag against the branch's commit (the short hash) when the tag contains a hash | `image_matches_branch`: `yes`, `no` or `unknown` |

Every row is labelled as a comparison of declared configuration, not a proof. A service with no matching Compose or Dockerfile node in the view gets "nothing declared to compare with".

## Tech Stack

- Rust 2024, `tokio`, `serde`, `serde_json`, `thiserror`.
- New dependency, needing approval: a regular-expression crate with linear-time matching (for example `regex`) for `match_log`, signatures and log parsing.
- The SSH adapter and the remote runner come from `contexts` (`lib/adapters`); this module adds no I/O of its own.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Fixtures: cargo test -p engine operations
Live:    cargo test --workspace -- --ignored    # against a real VM given by environment variables, read-only
```

## Project Structure

```
lib/engine/src/operations/
├── catalog.rs     # every CommandTemplate this module offers, as data
├── params.rs      # the validators (names, selector, duration, count)
├── k8s.rs         # parsing kubectl JSON into the views the tools return (values dropped here)
├── compose.rs     # parsing docker compose output the same way
├── logs.rs        # log line parsing, levels, filtering, cut rules
├── signature.rs   # message normalisation and grouping
├── trace.rs       # Python and Node stack traces, frames
├── findings.rs    # the fixed finding rules
├── drift.rs       # check_config comparison
└── resolve.rs     # service → pods, with the known-services list

services/cli/src/mcp/modules/operations.rs   # the seven tools
lib/engine/tests/fixtures/operations/        # recorded kubectl and compose outputs, with planted fake secrets
```

The engine has no I/O: the tools run over the `RemoteRunner` port from `contexts` and a fake that replays fixtures.

## Code Style

A template is plain data the catalog, the audit line and the tests can all read:

```rust
/// kubectl logs for one pod; every value is validated before this is built.
pub fn pod_logs(ns: &Namespace, pod: &PodName, window: &LogWindow) -> CommandTemplate {
    CommandTemplate::new("kubectl")
        .arg("logs")
        .arg(pod.as_str())
        .arg("-n")
        .arg(ns.as_str())
        .arg("--timestamps")
        .args(window.flags())
}
```

Follow existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine.

## Testing Strategy

- **Catalog:** a test lists every template and fails on any verb outside the allowed set; no template reads a Secret.
- **Validators:** each rule has valid and invalid cases; injection attempts (`;`, `&&`, `$(…)`, backticks, newlines, a leading `-`, `--kubeconfig=…` smuggled as a name, path traversal) are all refused before any command is built.
- **Parsing from fixtures:** recorded `kubectl` and Compose output with planted fake secrets (a literal `hunter2` environment value, a `secretKeyRef`, a ConfigMap value) produces answers that never contain the values (checked by searching the whole serialised answer).
- **Findings:** each rule has a fixture that triggers it and one that does not; each finding carries its evidence.
- **Logs:** JSON and plain levels; `pattern` and `level` filtering; the 5-pod, 2 000-line and 1 MB cuts with notices; runner truncation noticed.
- **Signatures and traces:** numbers, UUIDs and IDs normalise to one signature; "new vs previous window"; Python and Node traces parse; frames map to nodes in the context's view and not in `working`.
- **Exact-match scrub:** a resolved fake context secret planted in a log line is removed from the answer.
- **`check_config`:** missing and unexpected names, port mismatch, image `yes`/`no`/`unknown`, nothing declared.
- **Not configured and failures:** `kubectl` absent from the context answers "not configured"; an SSH failure names the context and host; an unknown service lists the known ones.
- **Audit:** every command run appears as a `remote_command` line (`SPEC-audit.md`) with the template, arguments, exit code and output size.
- **Concurrency:** more than 4 simultaneous commands queue.
- Every row of the tools table and the catalog has a test; the live test (`--ignored`) runs the read-only catalog against a real VM.

## Boundaries

- **Always:** run only catalog templates through the remote runner; validate every parameter; parse and reduce output before returning it; say which context answered; bound every answer and say when it was cut; read code only from the context's view.
- **Ask first:** adding a template or a `kubectl`/`docker` verb; returning any configuration value; reading a Secret; a second log source; letting a tool take a namespace; any pattern-based masking.
- **Never:** run a command that is not a catalog template; change an observed system; return an environment, ConfigMap or Secret value; pass a free-form string to the VM; open a shell, a TTY or `sudo`; run `kubectl` on the workstation; call an LLM.

## Success Criteria

1. Each of the seven tools answers from the active context through catalog templates only, and every answer names its context.
2. No parameter can add an argument, an operator or a path; invalid parameters are refused before any connection is used.
3. The catalog holds only read verbs, and a test fails if that changes.
4. No answer contains an environment, ConfigMap or Secret value, and no resolved context secret appears in any output.
5. `fetch_logs` and `match_log` bound time, pods, lines and bytes and say when they cut.
6. `analyze_logs` returns signatures with counts and "new" marks, stack traces with frames mapped to nodes in the context's branch view, and the pods' restarts.
7. `k8s_status` and `compose_status` return findings with evidence from the fixed rules.
8. `service_config` returns names and structure only; `check_config` reports the drift rows above and labels them as comparisons.
9. Every remote command is a `remote_command` line in the session log.
10. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Compose on the VM.** The context file has no place for the VM's Compose directory and file names. Proposed amendment to `SPEC-contexts.md`: an optional `compose:` section (`binary`, `project_dir`, `files`). Approve, or drop `compose_status` from the first release?
2. **One namespace per context.** Do you ever need a second namespace in one context? If so, an allow-list `kubectl.namespaces` in the context file, and a `namespace` argument limited to it.
3. **Logs are returned as written.** A token the application prints reaches the agent and the session log; only Workbench's own secrets are scrubbed. Is that acceptable for production logs, given the no-pattern-redaction decision?
4. **Names only for configuration.** Is it enough that `service_config` never shows values (not even non-secret ones like a feature flag)?
5. **Drop `lsp` from this module's dependencies** in the map, since only the graph is used?
6. **`check_config` image-vs-branch** relies on the image tag containing the commit hash and says `unknown` otherwise. Acceptable heuristic?
