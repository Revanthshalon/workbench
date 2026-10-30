# Spec: incidents

Module `incidents` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`storage`](SPEC-storage.md) (draft: the `IncidentStore` port), [`semantic-layer`](SPEC-semantic-layer.md) (draft: node addresses), and reads the session log ([`audit`](SPEC-audit.md), draft) and flows ([`flow-memory`](SPEC-flow-memory.md), draft). Used by `agent-prompts` (`write_rca`, `triage_logs`, `diagnose`). `operational-support` only.

## Decisions already made elsewhere

1. Tools: `save_incident_report` (kind `record`) and `find_incident_reports` (kind `observe`). The report follows `lib/engine/templates/incident-report.md`, the template the `write_rca` prompt fills in.
2. Reports are stored in the storage backend (`IncidentStore`, `SPEC-storage.md`).
3. Resolved secrets are removed from everything written by exact match; nothing is scrubbed by pattern (`SPEC-audit.md`). Incident text is sensitive and stays on the person's own machine (`SPEC-storage.md`, Assumption 7).
4. No tool changes an observed system; saving a report writes only to Workbench's own storage and files (`SPEC-core.md`).

## Assumptions (correct me now)

1. **A report is a Markdown document in the template's shape**, not a form. The agent (or a person, through the agent) writes the whole report; Workbench **validates the shape, reads out the fields it needs, and stores the document**. It never rewrites the prose.
2. **Validation reports every problem at once, with line numbers, and saves nothing if there is one:** an unreplaced `{{placeholder}}`; a missing numbered section (1 to 11) or the Appendix; a missing row in the header table; a `Status` that is not `investigating`, `mitigated`, `resolved`, `monitoring` or `closed`; a `Severity` that does not start with `SEV1` to `SEV4`; an `Environment` whose first word is not `production`, `staging` or `testing`; a `Detected` or `Started` time that is not ISO 8601 UTC; `Mitigated` and `Resolved` that are neither a time nor `not yet`; and an empty **Evidence** table (§5.4), because the template's rule is that every claim cites evidence. Whether the evidence is *true* is not checked.
3. **The guidance comments are stripped** (`<!-- … -->`), as the template says they must be before publishing, and the save reports how many.
4. **The incident id is assigned by Workbench.** On the first save the header's `Incident ID` may be left as the placeholder; Workbench fills it with `INC-YYYYMMDD-NN`: the date of `Detected` (the save date when that cannot be read) and the next number for that date in this workspace, two digits, three after 99. The storage enforces uniqueness, and a clash retries with the next number.
5. **A report changes over time, so every save is a revision.** Saving with an existing `id` stores revision *n+1*. The agent passes `base_revision`; if the report has moved on since it was read, the save is refused with the current revision, so two sessions cannot overwrite each other. Older revisions stay in the storage; the file holds the latest.
6. **Appendix A is written by Workbench, not by the agent.** On every save the table of tool calls is rebuilt from the `tool_call` and `tool_result` events of the session log (time, tool, key arguments cut at 120 characters, `ok` or `error`; at most 200 rows, newest first when more), replacing whatever the agent wrote there. The agent cannot edit the record of what it did.
7. **Log excerpts are allowed in Appendix B but bounded:** at most 200 lines and 32 KB in total; more is refused with the limit. They are not scrubbed except by the exact-match rule, so people should keep excerpts short (the template already says "redacted").
8. **Searchable fields are read from the document, not asked for again:** title (the first heading), status, severity, environment, services affected, the four times, the primary category and contributing factors (§5.1), confidence, the **log signatures** (§4.2 rows of kind "Log signature"), the summary (§1), the root cause statement (§5.2) and the runbook entry (§10). Signatures are also stored **normalised** with the same normaliser as `analyze_logs` (`SPEC-operations.md`), so a past report matches a signature found today.
9. **Code locations are linked.** `path:line` references in §5.3 and §5.4 are mapped to graph nodes in the context's branch view and stored as addresses (project, path, qualified name), so reports can be found by code location. A reference that maps to nothing is kept as text.
10. **Reports can cite flows.** `save_incident_report` takes optional `related_flows` (flow ids); an id that is not stored is refused. Related flows are shown with the report.
11. **Limits:** a report is at most 200 KB; `find_incident_reports` returns 10 results by default, 50 at most.
12. **A file copy** of the latest revision is written to `.workbench/incidents/<id>-<slug>.md` (temp file and rename, owner-only), because a report is something people read, share and keep. It is the report as written (comments stripped, id filled in, Appendix A rebuilt), with no extra front matter. The storage is the working store; storage and files are reconciled when an operational server starts, as for flows: a file for an id not in the storage is imported (and must pass validation, otherwise it is reported and skipped), a stored report without a file gets one, and an id in both is never overwritten from the file.
13. **The template changes to match the tools as specified** (it still uses early names: `ops-mcp`, `.ops/logs/audit.*.jsonl`, `datastore=`, `min_level`, `match_log(line=…)`, `contract(name=…)`, and the environment `local`). This change makes those edits to `lib/engine/templates/incident-report.md`: Workbench names, the session log path, `match_log(service, pattern)`, `analyze_logs(service, since)`, `datastore_health(system, connection)`, `db_query(connection, sql)`, `contract(project)` and `testing` instead of `local`.

## Objective

Keep each investigation as a report people can read and trust the shape of, and let the next investigation find the last one that looked the same.

The module is done when:
- a report that follows the template is saved, assigned an id, versioned and written as a file; one that does not is refused with every problem listed,
- past reports are found by text, service, status, severity, category, signature, time, code location or similarity to a signature,
- Appendix A is always the real record of the session's tool calls,
- the files can rebuild an empty storage, and two sessions cannot silently overwrite one another.

## The tools

| Tool (kind) | Input | Answer |
|---|---|---|
| `save_incident_report` (`record`) | `report` (the Markdown), `id` and `base_revision` (to revise), `related_flows`, `session` (default: this one) | On success: the id, revision, file path, the fields Workbench understood (title, status, severity, environment, services, category, signatures), and warnings (comments stripped, Appendix A rows written, code locations that mapped to nothing). On failure: the list of problems with line numbers, and nothing is stored |
| `find_incident_reports` (`observe`) | `id` (with optional `revision`: the whole report), or any of `query`, `service`, `status`, `severity`, `environment`, `category`, `signature`, `node` (a project, file or symbol), `similar_to` (a signature), `since` (on `Detected`), `limit` | Summaries newest first, or with `similar_to` ranked: id, revision, title, status, severity, environment, services, detected and resolved times, category, root cause statement (cut at 300 characters), signatures, related flows, file path, and which fields matched |

### Similarity

`similar_to` takes a log signature. It is deterministic: reports with the same normalised signature rank first; then those whose signature contains it or is contained in it; then those that share a service and the same primary category (`SPEC-flow-memory.md` uses the same style). Each result names the rule that matched. No model is involved.

## Save, step by step

```text
parse the document (headings, header table, section tables)
  → validate (Assumption 2); any problem → return all of them, store nothing
  → strip guidance comments; fill the id (first save); check base_revision (revisions)
  → rebuild Appendix A from the session log; check Appendix B limits
  → exact-match scrub of resolved secrets
  → read the searchable fields; normalise signatures; map code locations to node addresses
  → check related_flows exist
  → one storage write (IncidentStore), unique id and revision enforced there
  → write the file by temp file and rename
  → answer
```

If the file write fails after the storage write, the answer says the report is saved and names the file error; the next reconcile writes the file.

## Tech Stack

- Rust 2024, `tokio`, `serde`, `chrono` or `time` for the timestamps (whichever the workspace already uses), `thiserror`.
- No new dependencies. The Markdown is parsed in-house for headings and tables only, because the shape is fixed by the template.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Memgraph: cargo test --workspace -- --ignored    # the shared IncidentStore suite against a local Memgraph
```

## Project Structure

```
lib/engine/templates/incident-report.md   # the template (updated here); included in the binary

lib/engine/src/incidents/
├── model.rs        # Report, Revision, Fields, IncidentId
├── parse.rs        # headings, the header table, section tables, comments
├── validate.rs     # the rules of Assumption 2, every problem with its line
├── id.rs           # INC-YYYYMMDD-NN assignment
├── appendix.rs     # Appendix A from session events; Appendix B limits
├── fields.rs       # the searchable fields and normalised signatures
├── locations.rs    # path:line references → node addresses
├── similar.rs      # the similarity rules
├── find.rs         # filters and ranking over IncidentStore
├── reconcile.rs    # the storage ↔ files plan
└── ports.rs        # IncidentFiles (read/write the incidents folder), SessionLog (events)

lib/engine/src/storage/ports.rs   # IncidentStore: put_report, get_report, list_reports(filter), next_sequence(date)

lib/adapters/src/incidents/files.rs   # `.workbench/incidents/` with temp file, rename, owner-only permissions

services/cli/src/mcp/modules/incidents.rs   # the two tools
```

The engine has no I/O: parsing, validation, ids, appendix, fields, locations, similarity and reconcile are plain logic over `IncidentStore`, `IncidentFiles`, `SessionLog` and the graph.

## Code Style

Validation returns every problem so a person fixes the report once:

```rust
/// One problem found in a report.
#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub message: String,
}

pub fn validate(doc: &ParsedReport) -> Vec<Problem> {
    let mut problems = Vec::new();
    problems.extend(placeholders(doc));
    problems.extend(sections(doc));
    problems.extend(header_fields(doc));
    problems.extend(evidence(doc));
    problems
}
```

Follow existing conventions: one error enum per concern, doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **Template round trip:** the shipped template, with every placeholder replaced by fixture values, validates and parses to the expected fields; the shipped template as it is fails with the placeholders listed (a test that keeps the template and the validator in step).
- **Validation:** each rule has a failing and a passing document; several problems come back together with their lines; a failed save stores and writes nothing.
- **Ids and revisions:** the first save gets `INC-YYYYMMDD-NN` from `Detected`, falling back to the save date; the sequence increments per date and survives imports; a clash retries; a save with a stale `base_revision` is refused with the current one; old revisions are readable.
- **Appendix A:** rebuilt from a fixture session log whatever the agent wrote; cut at 200 rows; arguments cut at 120 characters; no result content. **Appendix B:** over 200 lines or 32 KB is refused.
- **Scrub:** a planted fake secret anywhere in the report is removed by the exact-match rule.
- **Fields and signatures:** the searchable fields are read from the document; a signature written in a report and the same message seen in `analyze_logs` normalise equal.
- **Locations:** `path:line` in §5.3 and §5.4 map to nodes in the context's view; an unmappable one stays text and is warned.
- **Related flows:** a stored flow id is accepted; an unknown one is refused by name.
- **Find:** each filter alone and combined; `since`; the limit and its notice; `id` with a revision; `similar_to` ranks by the three rules and names the rule.
- **Files:** the file equals the stored latest revision; owner-only; atomic replacement; reconcile writes missing files, imports valid unknown ones, skips and reports invalid ones, never overwrites a stored id.
- **Shared suite:** every `IncidentStore` operation against the in-memory fake and Memgraph.
- **Never calls production:** a fake remote runner and datastore client record zero calls during both tools.
- Every tool and every rule in Assumptions 2 to 12 has a test.

## Boundaries

- **Always:** validate the whole document and report every problem; rebuild Appendix A from the session log; keep every revision; enforce `base_revision`; write files by temp file and rename; remove resolved secrets by exact match.
- **Ask first:** deleting a report or a revision; letting the agent write Appendix A; changing the template's headings or the rules of Assumption 2 (they invalidate stored reports); sharing reports between people; a new report kind.
- **Never:** rewrite the prose of a report; store a report that fails validation; overwrite a stored report from its file; call a production system or a datastore; guess a root cause or fill a section for the agent.

## Success Criteria

1. A report that follows the template saves with an assigned id and revision and as a file; one that does not is refused with every problem and its line, and nothing is stored.
2. The shipped template matches the tool names in the specs, and with its placeholders replaced it validates.
3. Appendix A always equals the session log's tool calls, whatever the agent wrote.
4. Saving over a report that changed since it was read is refused with the current revision; all revisions stay readable.
5. `find_incident_reports` finds reports by text, service, status, severity, environment, category, signature, time and code location, and ranks `similar_to` by the three named rules.
6. Signatures normalise the same as `analyze_logs`, so an old report matches today's message.
7. After the storage is wiped, starting an operational server restores every valid report file; an invalid file is reported, never modified.
8. Neither tool makes a remote or datastore call, and no resolved secret appears in a stored report or file.
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **No confirmation to save.** A report may be saved at any status, including a draft while investigating. Should saving as `resolved` or `closed` need the user's confirmation (elicitation), as a flow does?
2. **Appendix A is generated, replacing the agent's text.** Right, or should the agent's own notes be kept beside it?
3. **No delete.** Reports and revisions cannot be deleted by a tool; a person can remove a file but it comes back at the next start. Is that the right first-release rule, or do you want a way to remove one?
4. **Environments are the three contexts.** The template said `local`; it now says `testing`. Is there a real use for `local` reports?
5. **Log excerpts in Appendix B** are allowed (bounded, exact-match scrub only), so personal data could end up in a report. Forbid excerpts, or accept as written?
6. **The id's date is the `Detected` date.** Or always the save date?
