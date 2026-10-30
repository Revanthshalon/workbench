# Spec: semantic-layer

Module `semantic-layer` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`graph`](SPEC-graph.md) (draft) and [`storage`](SPEC-storage.md) (draft), and reads `semantic.llm` from the workspace config ([`workspace`](SPEC-workspace.md), approved). Used by `dev-support` (`project_context`), `flow-memory`, `incidents`, and the CLI (`workbench semantic …`, `workbench index --annotate`).

## Decisions already made elsewhere

1. **Annotations** attach meaning to AST nodes. Kinds: `purpose`, `role`, `domain-term`, `tag`, `link` (`SPEC-cli.md`, Further decision 7). Each is written by a **person** or an **agent**. An agent cannot overwrite or remove a person's annotation.
2. Annotations are **flagged stale** when their node changes and **orphaned** when it disappears; they are never silently dropped. `prune` removes them only when a person runs it.
3. **People's annotations are mirrored** to `.workbench/memory/<project-id>/annotations.yml`, so they survive a wiped Memgraph. Agent-written ones are not mirrored, because they can be regenerated.
4. **LLM drafting is opt-in twice**: `semantic.llm` in the workspace config **and** `workbench semantic generate` or `workbench index --annotate`. Providers: `copilot-cli`, `ollama`, `claude`. Only `claude` needs a key, read from the variable named in `api_key_env` only when a call is made, held as a `secrecy` secret. Nothing is sent to an LLM otherwise. A run sends only the source of indexed nodes, never other files, never overwrites a person's annotation, and records its output as agent-written with the model name (`SPEC-cli.md`).
5. **Remembered knowledge and code search** span all projects by default and can be narrowed to a scope. Search is a case-insensitive contains-scan with a result limit (`SPEC-storage.md`, Assumption 6).
6. Tools (kind `record` for the writers, `observe` for the readers): `remember`, `revise`, `retract`, `recall`, `knowledge_gaps`, `search_code`. They exist in both modes; in `operational-support` they see the session's context view.
7. `INIT-MEMORY.md` is human-written text. This module reads it and may suggest additions; nothing overwrites what a person wrote.

## Assumptions (correct me now)

1. **An annotation is anchored to an address, not to a view.** A node's id starts with its view (`SPEC-graph.md`), but an annotation records `project`, `path` and `qualified name` (the id without the view), so it follows the same code across `working` and the context branches. Whether it is current, stale or orphaned is worked out **against the view being read**, so an annotation can be current on `working` and stale on `context:production`, which is true and useful.
2. **Staleness is a fingerprint comparison.** The graph stores a `fingerprint` on each node (defined below). An annotation keeps the fingerprint of the node as it was when the annotation was written or last confirmed. Different fingerprint: **stale**. No node at the address: **orphaned**. A stale or orphaned annotation keeps the last-seen signature, so it still reads sensibly.
3. **Fingerprints ignore noise a person would ignore.** A function or method: its source with each line trimmed and blank lines removed. A class, component, service or directive: its header (name, bases, decorators) and the signatures of its members, **not** their bodies, so editing a method does not stale the class's annotation. A file: its top-level names and signatures. A project is never stale. Comments count (changing a docstring makes an annotation stale; confirming it is one command).
4. **At most one current `purpose` and one current `role` per node.** A second one is refused with the id of the existing one, so `revise` is the way to change it. `domain-term`, `tag` and `link` may repeat. Texts: at most 500 characters for annotations; a `link` points at another node (project, file or symbol) in the same view.
5. **Person-written beats agent-written.** A person revising an agent annotation's text turns it into a person annotation (it gets mirrored); confirming it without new text keeps its author. A person's `revise`, `retract` and `prune` act on any annotation; an agent's act only on agent annotations.
6. **Remembered knowledge is a separate thing: notes.** A note is free text about a project or the whole workspace (a decision, a gotcha, how something works) with a title and tags, not tied to a node. The per-project and workspace `INIT-MEMORY.md` files are **seeded** into notes, split by heading, marked `source: init-memory` and read-only; they are replaced when the file's hash changes. Notes written through `remember` are agent-written and are **not** mirrored (Open Question 2).
7. **`search_code` searches the graph, not file text.** It scans node names, qualified names, signatures, docstrings and annotation text. Raw text search is what an AI platform's own tools do. (Open Question 3.)
8. **LLM drafting is bounded.** Only `Project`, `File`, `Class`, `Function`, `Method`, `Component`, `Service` and `Directive` nodes are eligible (never YAML, Compose, Dockerfile or environment nodes). The prompt holds the node kind, path, signature, the body (at most 200 lines), the enclosing class's signature and nothing else. The answer must be JSON with `purpose` (required), optional `role`, `domain_terms` and `tags`, and every text is capped and validated; a bad answer is dropped and counted. Two calls at a time, 60 seconds each, one retry.
9. An HTTP client is a new dependency (for `ollama` and `claude`) and needs approval. `copilot-cli` runs the `copilot` program as a subprocess.
10. Annotation ids look like `ann_` plus eight random hexadecimal characters and never change. Note ids look like `note_` plus eight.

## Objective

Let people and agents add meaning to code that no parser can see (why it exists, what a domain word means, how things relate), keep it trustworthy as the code changes, and make it findable.

The module is done when:
- an annotation can be added, revised, confirmed, retracted, listed, shown, pruned and drafted by an LLM, by people through the CLI and by agents through the tools, with the authority rules above,
- every annotation reports `current`, `stale` or `orphaned` for the view being read, and none is lost when code changes,
- a person's annotations survive wiping Memgraph, because `workbench index` restores them from the mirror,
- notes and annotations can be recalled and searched across projects, and `knowledge_gaps` points at what is undocumented.

## The model

```text
Annotation { id, kind, project, path?, symbol?, text, to? {project, path?, symbol?},
             author (person | agent{model}), fingerprint, last_seen {signature, kind},
             created, revised }
Note       { id, project? (none = workspace), title, text, tags, source (agent | init-memory), created, revised }
State      = current | stale | orphaned     (never stored: worked out for a view)
```

An annotation with no `path` is on the project; with `path` and no `symbol`, on the file.

### Staleness and the graph

On every indexing run nothing is rewritten in annotations. State is computed when read:

```text
for each annotation, in the view being read:
  node = graph.node_at(view, project, path, symbol)
  none                         → orphaned
  node.fingerprint != annot's  → stale
  else                         → current
```

`graph` gains the `fingerprint` property on code nodes and the lookup `node_at(view, project, path, symbol)` (amendment to `SPEC-graph.md`, made in this change). `prune` evaluates against the `working` view only.

### The mirror

`.workbench/memory/<project-id>/annotations.yml`, written whenever a person's annotation changes (`add`, `revise`, `retract`, `prune`, and a person's confirmation), through a temporary file and a rename, under one file lock. A link lives in the folder of its source project.

```yaml
version: 1
annotations:
  - id: ann_3f9a1c2e
    kind: purpose
    target: {path: app/invoices.py, symbol: InvoiceService.create}   # project = the folder
    text: Creates an invoice and reserves stock; the only writer of the invoices table.
    author: person
    fingerprint: 9b1c40a7d2e3f568
    last_seen: {kind: Method, signature: "create(self, order_id: int) -> Invoice"}
    created: 2026-10-30T10:00:00Z
    revised: 2026-10-30T10:00:00Z
```

**Restore:** each `workbench index` run (and `watch` start) imports every mirror entry whose id is not in the storage. An id that exists is never overwritten by the mirror (the storage is newer), and a retracted annotation is gone from the mirror too, so nothing comes back from the dead. Entries with a missing project or an unreadable file are reported and skipped; the file is never modified by an import.

## The tools

| Tool (kind) | Input | Answer |
|---|---|---|
| `remember` (`record`) | `kind` (`purpose`, `role`, `domain-term`, `tag`, `link`, `note`), `text`, `project`, and for annotations `file`/`symbol` (and `to_*` for `link`); for `note`: `title`, `tags` | The new id and, for an annotation, its state. Fails if the target is not in the index (suggest `reindex`), or a `purpose`/`role` exists (names it) |
| `revise` (`record`) | `id`, `text` or `confirm: true` | The updated item. Refused on a person's annotation, with the author named |
| `retract` (`record`) | `id` | Confirmation. Refused on a person's annotation |
| `recall` (`observe`) | `query`, `projects`, `kind`, `node` (a target), `states` (default `current`, `stale`), `limit` | Matching annotations and notes, each with id, author, state and for annotations the node it is on |
| `knowledge_gaps` (`observe`) | `projects`, `limit` | A ranked list: nodes with no annotation that many others depend on (routes, entry points, most-depended-on classes), stale and orphaned annotations, and projects whose `INIT-MEMORY.md` is still an empty template; each with a reason |
| `search_code` (`observe`) | `query`, `projects`, `kind`, `limit` | Nodes whose name, signature, docstring or annotations contain the query: id, kind, project, path, line, matching annotation text |

`project_context` (in `dev-support`) takes its annotation highlights from this module: the project's purpose and role, up to 20 domain terms, and the purposes of up to 10 most-depended-on nodes, with their state.

## CLI

The commands are specified in `SPEC-cli.md` and unchanged. This spec adds behaviour: `show` and `list` print the state per annotation; `revise` without `--text` re-anchors (a new fingerprint and last-seen signature); `prune` and `generate` act as described under Decisions and "LLM drafting".

### LLM drafting

```text
select eligible nodes (no annotation, or only stale agent ones), cap with --max-nodes
  → build the prompt (Assumption 8)         nothing else leaves the machine
  → provider call (copilot-cli | ollama | claude), 2 at a time, 60 s, one retry
  → validate the JSON, drop bad answers (counted)
  → write as agent annotations with the model name; never touch a person's
  → print: nodes sent, provider, model, created, dropped
```

Preflight failures (no config, missing key, `copilot` not found, endpoint unreachable) stop the run before any call and name the cause. `--dry-run` shows the node count and roughly how much text would be sent and calls nothing. Code is sent to the provider as written, so a secret inside a function body would be sent too; only the node kinds above are eligible and `.env`-style files are never indexed, but this is stated plainly in the command's help.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `yaml_serde`, `secrecy`, `thiserror`.
- New dependency, needing approval: an HTTP client (for example `reqwest`) for the `ollama` and `claude` providers.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Try:     cargo run -p workbench -- semantic add --project billing-api --file app/invoices.py --symbol InvoiceService.create --kind purpose --text "Creates an invoice"
         cargo run -p workbench -- semantic list --stale
         cargo run -p workbench -- semantic generate --project billing-api --dry-run
```

## Project Structure

```
lib/engine/src/semantic/
├── model.rs        # Annotation, Note, Kind, Author, State, Target
├── state.rs        # fingerprint comparison, state for a view
├── rules.rs        # authority rules, one purpose/role per node, text limits
├── mirror.rs       # the annotations.yml model, serialising, the restore plan
├── notes.rs        # splitting INIT-MEMORY.md into seed notes
├── gaps.rs         # knowledge_gaps ranking
├── search.rs       # recall and search_code over the ports
├── draft.rs        # eligibility, the prompt, validating the model's answer
└── ports.rs        # AnnotationDrafter (the LLM), MirrorStore (read/write annotations.yml)

lib/engine/src/storage/ports.rs   # KnowledgeStore, SearchIndex (operations below)

lib/adapters/src/semantic/
├── mirror.rs       # annotations.yml with temp file + rename + lock
└── drafter/        # copilot_cli.rs, ollama.rs, claude.rs

services/cli/src/commands/semantic.rs   # `workbench semantic …`
services/cli/src/mcp/modules/semantic.rs   # the tools
```

**Port operations** (added to `SPEC-storage.md`): `KnowledgeStore`: `put_annotation`, `get`, `list(filter)`, `delete`, `put_note`, `list_notes`, `replace_seed_notes(project, notes)`. `SearchIndex`: `search_nodes(query, projects, kind, limit)` and `search_annotations(query, projects, limit)`. Each has a test in the shared suite.

## Code Style

```rust
/// Where an annotation stands against the code in one view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Current,
    Stale,
    Orphaned,
}

pub fn state(annotation: &Annotation, node: Option<&NodeFingerprint>) -> State {
    match node {
        None => State::Orphaned,
        Some(n) if n.fingerprint != annotation.fingerprint => State::Stale,
        Some(_) => State::Current,
    }
}
```

Follow existing conventions: doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **State:** unchanged node is current; changed body is stale; renamed or deleted is orphaned; the same annotation is current on one view and stale on another; editing a method does not stale its class; a docstring edit stales its function.
- **Rules:** a second `purpose`/`role` is refused naming the first; an agent cannot revise or retract a person's annotation; a person's revision of an agent annotation makes it person-written; text limits; a `link` needs a target in the index and is rejected elsewhere.
- **Mirror:** a person's write produces the exact file shown above; an agent's write produces none; a crash between write and rename leaves the old file; restore imports only missing ids and never overwrites; a retracted annotation is not restored; a bad entry is skipped and reported; the file is not changed by an import.
- **Notes:** `INIT-MEMORY.md` splits by heading into seed notes, replaced when the hash changes, and a seed note cannot be revised or retracted.
- **Recall, search and gaps:** scope narrowing, case-insensitive contains, limits and truncation notice; `knowledge_gaps` ranks an undocumented, much-depended-on node above a leaf and reports an empty `INIT-MEMORY.md`.
- **Drafting:** a fake `AnnotationDrafter`: eligibility (ineligible kinds never sent), the prompt holds exactly the allowed parts, a bad JSON answer is dropped and counted, a person's annotation is never touched, `--dry-run` calls nothing, preflight failures name the cause, the API key never appears in output (a fake key in the environment).
- **Shared suite:** each `KnowledgeStore` and `SearchIndex` operation against the fake and Memgraph.
- **Integration** (`services/cli/tests/`): `add` then `list`; change the code, `index`, `list --stale`; wipe the storage, `index`, annotations are back; `prune --dry-run`; a tool call by an agent against a person's annotation is refused.
- Every row of the tools table has a test.

## Boundaries

- **Always:** compute state against the view being read; keep a person's annotations in the mirror; keep the last-seen signature on stale and orphaned ones; check the target exists before attaching; send an LLM only the allowed parts of eligible nodes.
- **Ask first:** adding a provider or a dependency; changing what is sent to an LLM; mirroring agent annotations or notes; changing the fingerprint rules (it makes every annotation stale at once); new annotation kinds.
- **Never:** drop an annotation silently; let an agent change a person's annotation; write into a project's source files or overwrite `INIT-MEMORY.md`; call an LLM without both the config and the flag; store or print the API key; let a mirror import overwrite what is in the storage.

## Success Criteria

1. `workbench semantic add | revise | retract | list | show | prune | generate` behave as in `SPEC-cli.md` with the authority rules above.
2. State is `current`, `stale` or `orphaned` per view, from the fingerprint, and a class annotation survives edits to its methods' bodies.
3. After `workbench index` on a wiped Memgraph, every person's annotation is back and nothing retracted returns; agent annotations are not.
4. The tools `remember`, `revise`, `retract`, `recall`, `knowledge_gaps` and `search_code` return the answers described, scoped to the requested projects and the session's view.
5. An agent cannot revise or retract a person's annotation, and only one current `purpose` and `role` exist per node.
6. LLM drafting runs only with config and flag, sends only the allowed parts of eligible nodes, records agent annotations with the model name, and never prints the key.
7. `INIT-MEMORY.md` content is recallable as read-only seed notes and is never modified.
8. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **Restore from the mirror on every `index`.** Is that the right trigger, or should restoring be an explicit step?
2. **Notes are not mirrored.** They are agent-written and regenerable, and people write `INIT-MEMORY.md`. Should person-visible notes also go to a `notes.yml`?
3. **`search_code` covers names, signatures, docstrings and annotations, not file text.** Enough for the first release?
4. **Class fingerprints ignore method bodies** (Assumption 3). Right trade-off between noise and missed changes?
5. **Orphans cannot be moved.** A renamed symbol's annotation is orphaned; a person retracts and re-adds. Should a `semantic move <ID> --to …` command be added to `SPEC-cli.md`?
