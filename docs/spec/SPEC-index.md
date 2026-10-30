# Spec: index

Module `index` from the [capability map](capability-map.md). Status: **draft, awaiting review.**

Depends on [`graph`](SPEC-graph.md) (draft, defines what a parse produces), [`storage`](SPEC-storage.md) (draft, the lock and the atomic writes) and [`cli`](SPEC-cli.md) (approved, the command surface). Used by `workbench index`, `workbench watch`, the MCP `reindex` and `index_status` tools, the `edit` tools (`SPEC-core.md`), `workbench context sync` (`SPEC-contexts.md`) and `workbench project add --index`. `semantic-layer` hooks in after a run (mirror restore, `--annotate`).

## Decisions already made elsewhere

1. **Incremental by default:** only files that changed since the last run are re-parsed, and what depends on them is re-linked, never the whole codebase (`SPEC-cli.md`). `--changed` is a shortcut using git; `--full` re-parses everything; content hashes are the source of truth.
2. **One use case, many callers.** `workbench index`, the MCP `reindex` tool, the watcher, `context sync`, `project add --index` and the `edit` tools all call the same engine use case, so they give the same result.
3. **One writer at a time:** one shared lock, `.workbench/run/index.lock`, covers every indexing run in the workspace; others wait (`SPEC-storage.md`, `SPEC-cli.md`).
4. **The index reads project files and writes only to the storage.** It never changes a project's files, never calls an LLM unless `--annotate` is given, and needs no secrets.
5. **Views:** a run indexes one view: `working` (the project's own path, used by `developer-support` and the CLI) or `context:<name>` (that context's worktree, `SPEC-contexts.md`).
6. The watcher decides **when** to run; this spec decides **what** a run does (`SPEC-cli.md`).
7. Each indexed file keeps its content hash and the parser version, so unchanged files are skipped (`SPEC-storage.md`, "What is stored").
8. After a run the semantic layer restores missing person-written annotations from the mirror and counts annotations that became stale or orphaned (`SPEC-semantic-layer.md`).

## Assumptions (correct me now)

1. **A file is unchanged when its SHA-256 and the parser version both match** what the partition stored. Every candidate file is read and hashed on a default run; there is no mtime shortcut. That is fast enough for the first release; an mtime and size pre-check is the upgrade if a very large project is slow.
2. **Discovery** walks the project path and honours the project's `.gitignore` and the built-in exclusions the watcher uses (`.git/`, `.workbench/`, `node_modules`, `.venv`, `target`, `dist`, `__pycache__`). Only supported file kinds are candidates (`SPEC-lsp.md` file kinds). Symlinks are not followed outside the project. A file that is not a candidate any more counts as removed.
3. **`--changed` diffs from the commit last indexed.** Each partition stores `indexed_commit` (the project's `HEAD` when its last run ended, none for a non-git project). `--changed` considers the files git reports changed between that commit and `HEAD`, plus uncommitted and untracked files, plus files already recorded as failed. It still hashes those candidates. Without an `indexed_commit`, or for a non-git project, it behaves like the default run. After a `git pull` it therefore sees what the pull brought in, which "changed against `HEAD`" would miss.
4. **Re-linking follows the file's exported surface.** After a changed file is parsed, its **surface** (the ids and signatures of its top-level nodes) is compared with the old one. If the surface is unchanged nothing else is touched. If it changed, the files that have `IMPORTS` edges into it are **re-parsed** (not re-resolved from stored data), and if one of those changes its own surface (a re-export such as TypeScript `export *` or Python `from x import *`) the step repeats for its importers. Cost is bounded by the dependents of files whose surface changed.
5. **A run applies one project at a time, atomically.** A project's changes (upserts, removals, the new `indexed_commit`) are one atomic write to `GraphStore`. A failure in one project does not undo another's. A project's overview inputs are recomputed after its write.
6. **The overview is recomputed whenever any project's partition changed in the run** (and on `--full`), as one replacement per view. It reads the already-stored partitions, so its cost is small next to parsing.
7. **A file that fails to parse keeps its previous nodes.** An unreadable file, invalid UTF-8 or an internal parser error records `index_error` on the file's node and leaves its last good nodes in place; its hash is not updated, so the next run retries it. A syntax error is not a failure: tree-sitter recovers and the file is stored as `partial`.
8. **The lock is waited for.** A caller waits up to 60 seconds (`--wait <SECONDS>` overrides it); then it fails naming the holder's pid and start time, which the lock file records. The `edit` tools wait only 10 seconds (Open Question 3).
9. **A project that left the config is cleaned up.** An unscoped run (no `--project`) deletes the partitions of projects that are no longer in the workspace config, in every view, and the overview edges pointing at them, and says so.
10. **Parsing is bounded:** CPU-bound parsing runs on blocking threads, at most one per core. A file over the size limit is `skipped` with a reason (`SPEC-graph.md`). Ctrl-C finishes the project whose write is in progress, skips the rest and exits `1` with "interrupted"; nothing is half-written.
11. `ignore`-style `.gitignore` handling is a new dependency and needs approval; the lock uses the standard library's file locking if the toolchain provides it, otherwise a small crate (also needs approval).

## Objective

Keep the graph true to the code on disk, cheaply, from every place that changes code, without anyone worrying about the order things ran in.

The module is done when:
- the same use case serves `index`, `reindex`, the watcher, `context sync`, `project add --index` and the `edit` tools,
- a second run on unchanged code parses nothing, and a one-line edit parses one file plus only the dependents of a changed surface,
- a failure in one file or project never loses previously indexed data or another project's,
- two runs never write at once, and the status tool says how far behind the index is.

## The use case

```rust
pub struct IndexRequest {
    pub view: ViewId,
    pub projects: Scope,        // all, or a list of ids (SPEC-workspace.md)
    pub mode: Mode,
    pub annotate: bool,
}

pub enum Mode {
    Incremental,              // default: hash every candidate
    Changed,                  // git shortcut (Assumption 3)
    Full,                     // ignore stored hashes
    Files(Vec<PathBuf>),      // a targeted run: only these paths (edit tools, the watcher)
}
```

```text
validate scope (unknown id → fail with the known ids)
  → take the lock (wait up to --wait)
  → per project, in order:
       discover candidates      (Files(paths): just those, still only supported kinds)
       read the partition's index   (hashes, parser versions, surfaces, indexed_commit)
       decide: unchanged | parse | remove       (Full: everything is parse)
       parse changed files in parallel          (pure; graph module)
       surfaces changed → add importers, repeat
       one atomic write: upserts, removals, indexed_commit
       count: parsed, unchanged, removed, skipped, partial, failed, relinked,
              annotations that became stale or orphaned (working view only)
  → unscoped run: delete partitions of projects no longer in the config
  → if any partition changed (or Full): recompute and replace the view's overview
  → semantic layer: restore missing person-written annotations from the mirror
  → release the lock
  → if annotate: draft annotations for new or changed nodes without one (SPEC-semantic-layer.md)
  → report
```

`Files(paths)` skips discovery but not the rest: hashing, parsing, re-linking and the atomic write are the same. A path that no longer exists is a removal; a path outside the project or in an excluded folder is ignored and counted.

### The report

Per project: files `parsed`, `unchanged`, `removed`, `skipped` (with reasons), `partial`, `failed` (with the error), nodes `relinked`, annotations `became stale` and `orphaned`; and overall: duration, whether the overview was replaced, partitions deleted, annotations restored from the mirror. `workbench index` prints it (progress per project on stderr, the summary on stdout); `reindex` returns it. **Exit code `0` when no file failed; `1` if any file failed, any project could not be written, the lock could not be taken, or the run was interrupted.** `skipped` and `partial` are not failures.

### Callers

| Caller | Request |
|---|---|
| `workbench index [--project …] [--changed] [--full] [--annotate]` | `view = working`; `Incremental`, `Changed` or `Full` |
| MCP `reindex` (`projects`, `full`) | the session's view; `Incremental` or `Full` |
| `workbench project add --index` | `working`, that project, `Incremental`; a failure does not undo the add |
| `workbench context sync` | `context:<name>`, the context's projects, `Incremental` (after the worktree is at the branch's commit) |
| `edit` tools (`apply_patch`, `create_file`, `rename_symbol`, `apply_code_action`) | `working`, the touched files, `Files(paths)` |
| the watcher | `working`, see below |

### Edit tools and the lock

An `edit` tool's edit is already on disk when it asks for indexing. It waits up to 10 seconds for the lock; if it cannot get it, the tool still reports the edit as applied and adds `indexed: false`, and the watcher or the next run picks the files up. The tool answer never fails because of indexing.

### The watcher

The watcher turns file events into requests, nothing else:

| Situation | Request |
|---|---|
| A batch of up to 500 paths after the 500 ms delay | `Files(paths)` for each affected project |
| More than 500 paths, an overflow of the operating system's event queue, a change to `.gitignore`, a Compose file or a dependency lockfile, or a changed `HEAD` (a branch switch or pull) | `Incremental` for the affected projects |
| `watch flush` | whatever is pending, now, and it waits until done; with nothing pending and no watcher, one `Incremental` run |

## `index_status`

An `observe` tool in the context group (`SPEC-core.md`), also available as a line in `workbench storage status` per view. Per project and view it reports: files indexed, nodes, time of the last run and its result (failed files), `indexed_commit` against the project's current `HEAD`, a **behind** count and the watcher's state (running, last error, changes waiting).

**Behind** is computed without hashing, so it is cheap: for a git project, the files git reports changed since `indexed_commit` plus uncommitted and untracked ones, plus files recorded as failed; for a project that is not a git repository it says `unknown`. The graph tools use the same number for their "pending files" note (`SPEC-graph.md`).

## Tech Stack

- Rust 2024, `tokio` (blocking tasks, `fs`), `sha2`, `thiserror`.
- New dependencies, needing approval: `.gitignore`-aware directory walking (for example `ignore`), a file-watching crate for the watcher (named in `SPEC-cli.md`'s adapter), and a file-lock crate only if the standard library's is unavailable.
- `git` is used as a program for `--changed` and `indexed_commit`; it must be on `PATH`, and its absence only disables the git shortcut.

## Commands

```
Build:   cargo build --workspace
Test:    cargo test --workspace
Lint:    cargo clippy --workspace --all-targets -- -D warnings
Format:  cargo fmt --all
Index:   cargo run -p workbench -- index
Changed: cargo run -p workbench -- index --changed --project billing-api
Full:    cargo run -p workbench -- index --full
Wait:    cargo run -p workbench -- index --wait 120
```

## Project Structure

```
lib/engine/src/index/
├── request.rs     # IndexRequest, Mode, Scope handling
├── discover.rs    # candidate files: kinds, exclusions, removals
├── plan.rs        # unchanged | parse | remove, from stored hashes and parser version (pure)
├── surface.rs     # a file's surface and the re-link closure over IMPORTS
├── run.rs         # the use case: lock, per-project loop, overview, hooks, report
├── report.rs      # IndexReport, ProjectReport, exit rule
└── status.rs      # the behind count and IndexStatus

lib/engine/src/ports.rs   # + FileSource (list and read files of a view), ChangeSource (git: changed paths since a commit, HEAD), IndexLock

lib/adapters/src/index/
├── files.rs       # the walker and reader over a directory (project path or worktree)
├── git.rs         # ChangeSource over the git program
└── lock.rs        # IndexLock over .workbench/run/index.lock (pid and start recorded)

services/cli/src/commands/index.rs      # `workbench index`
services/cli/src/mcp/modules/index.rs   # `reindex`, `index_status`
```

Also changed by this spec: `SPEC-graph.md` (the `GraphStore` write operation, below) and `SPEC-cli.md` (the report line and `--changed`, `--wait`).

**`GraphStore` change** (amendment to `SPEC-graph.md`): the write for a run is `apply_changes(view, project, ChangeSet { upserts, removals, indexed_commit })`, one atomic write; `write_partition` stays for `--full` rebuilds and for tests. `read_partition_index` returns, per file, `hash`, `parser_version`, `surface` and any `index_error`, and the partition's `indexed_commit`.

The engine has no I/O: planning, surfaces, the closure, the report and the use case are plain logic over `FileSource`, `ChangeSource`, `IndexLock`, `SourceParser` and `GraphStore`.

## Code Style

```rust
/// What to do with one candidate file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Unchanged,
    Parse,
    Remove,
}

pub fn decide(stored: Option<&StoredFile>, hash: &Hash, parser_version: u32) -> Decision {
    match stored {
        Some(f) if f.hash == *hash && f.parser_version == parser_version && f.index_error.is_none() => {
            Decision::Unchanged
        }
        _ => Decision::Parse,
    }
}
```

Follow existing conventions: doc comments on every public item, no `unwrap` or `panic`, ports in the engine, adapters in `lib/adapters`.

## Testing Strategy

- **Planning:** same hash and parser version is unchanged; a different hash, a bumped parser version or a recorded error is parse; a stored file missing from discovery is removed; `Full` parses everything.
- **Surface and re-link:** an edit inside a function body re-parses one file; adding an exported function re-parses its importers; a re-export chain repeats the step; a cycle terminates.
- **Failure handling:** an unreadable file and invalid UTF-8 are `failed`, keep their old nodes and are retried next run; a syntax error is `partial`, not failed; one project's failed write leaves another's intact; the exit code follows the rule.
- **`--changed`:** from `indexed_commit` it sees files a pull brought in, uncommitted and untracked files and recorded failures; with no `indexed_commit`, or no git, it equals the default run.
- **`Files(paths)`:** only those paths are hashed; a deleted path is a removal; an excluded or foreign path is ignored and counted.
- **Lock** (adapter, real files): two runs never overlap; the second waits and then runs; after `--wait` it fails naming the holder; a stale lock file from a dead process is taken over; the `edit` path times out after 10 s and the tool still succeeds with `indexed: false`.
- **Overview and cleanup:** the overview is replaced only when something changed; a project removed from the config loses its partitions and the overview edges pointing at it, only on an unscoped run.
- **Idempotence:** a second run on unchanged code reports everything unchanged and writes nothing.
- **`index_status`:** behind counts files changed since `indexed_commit`, uncommitted, untracked and failed ones, without hashing; `unknown` for a non-git project.
- **Watcher mapping** (pure): a burst becomes one `Files` request; over 500 paths, an overflow, a Compose or `.gitignore` change or a `HEAD` change becomes `Incremental`.
- **Integration** (`services/cli/tests/`): the real binary with a fake `git` in a temp workspace: `init`, `project add`, `index`, edit a file, `index` parses one file; `index --full`; `index` with Memgraph down fails with the storage hint.
- Every row of the callers table has a test.

## Boundaries

- **Always:** take the lock before writing; write a project's changes in one atomic batch; keep the last good data for a file that fails; hash content, with git only as a shortcut for choosing candidates; say what was skipped and why.
- **Ask first:** adding a dependency (directory walking, file locking, file watching); changing what makes a file "unchanged" (it re-parses everything); an mtime shortcut; indexing a view other than `working` or a context's worktree.
- **Never:** change a project's files, branch or git state; run `git fetch`, `checkout` or anything that writes to a repository; write to the graph without the lock; leave a project half-written; call an LLM without `--annotate`; delete a partition of a project that is still in the config.

## Success Criteria

1. A second `workbench index` on unchanged code parses no file and writes nothing; editing one function body parses that file only; adding an exported symbol also re-parses the files that import it.
2. `--full` re-parses every file; `--changed` finds what a `git pull` brought in and what is uncommitted, and equals a default run without git.
3. A file that cannot be read keeps its old nodes and is retried; a syntax error is stored as `partial`; one project failing leaves the others written.
4. Two indexing runs, from any callers, never write at once; a waiting caller runs afterwards or fails with the holder's pid after `--wait`.
5. The `edit` tools index the files they touched, and still succeed (with `indexed: false`) when the lock is busy for more than 10 seconds.
6. The watcher's batches map to `Files` or `Incremental` requests as tabled, and `watch flush` waits for them.
7. `context sync` indexes the context's view; the working view is untouched.
8. An unscoped run removes the partitions of projects that left the config; a scoped one never does.
9. The report lists parsed, unchanged, removed, skipped, partial and failed files and the annotations that became stale or orphaned; the exit code is `1` only for the failure cases above.
10. `index_status` reports files, last run, `indexed_commit` against `HEAD` and a behind count without hashing.
11. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass.

## Open Questions

1. **`--changed` diffs from the last indexed commit** (Assumption 3) rather than against `HEAD`. Right?
2. **Lock wait of 60 seconds** for commands and `reindex`. Right default?
3. **The `edit` tools wait only 10 seconds** and then report `indexed: false`. Better to wait longer, or to fail the edit? (Proposal: neither; the edit stands.)
4. **An unscoped run deletes the partitions of projects that left the config.** Acceptable, or only on an explicit command?
5. **A file that fails to parse keeps its old nodes.** Alternative: remove them. Keeping them risks stale answers; removing them loses answers over a transient error.
