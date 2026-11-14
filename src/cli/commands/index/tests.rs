use std::{
    cell::RefCell,
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::{self, Command},
    sync::atomic::AtomicBool,
};

use clap::Parser as _;

use super::{
    Context, IndexArgs, content_hash, execute,
    parser::{ParseOutcome, Parser, TextParser},
    report::{Counts, IndexRun, ProjectReport, Status},
    storage::{ProjectChanges, Storage, StorageError, WriteResult},
};
use crate::cli::{
    Cli, Commands,
    errors::{CliErrors, CliResult},
    log::Logger,
    workspace,
};

const TWO_PROJECTS: &str = "version: 1
projects:
  - id: api
    path: api
    language: python
    environment:
      venv: env
  - id: web
    path: web
    language: typescript
";

/// A scratch workspace that is removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str, config: &str) -> CliResult<Self> {
        let scratch =
            Self(env::temp_dir().join(format!("workbench-index-{name}-{}", process::id())));
        let _ = fs::remove_dir_all(&scratch.0);
        scratch.write(".workbench/config.yml", config)?;
        Ok(scratch)
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> CliResult<()> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(fs::write(path, contents)?)
    }

    fn workspace(&self) -> CliResult<workspace::Workspace> {
        workspace::Workspace::find(&self.0)
    }

    fn index_with(
        &self,
        args: &IndexArgs,
        storage: &dyn Storage,
        parser: &dyn Parser,
        interrupted: bool,
    ) -> CliResult<IndexRun> {
        let workspace = self.workspace()?;
        let logger = Logger::new(workspace.root(), "index");
        let interrupted = AtomicBool::new(interrupted);
        execute(
            args,
            &Context {
                workspace: &workspace,
                storage,
                parser,
                logger: &logger,
                interrupted: &interrupted,
                progress: false,
            },
        )
    }

    fn index(&self, args: &IndexArgs) -> CliResult<IndexRun> {
        self.index_with(args, &FakeStorage::default(), &TextParser, false)
    }

    fn log(&self) -> String {
        let directory = self.0.join(".workbench/logs");
        fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Storage that starts from fixed hashes and records what is written.
#[derive(Default)]
struct FakeStorage {
    stored: HashMap<String, HashMap<String, u64>>,
    fail_writes: bool,
    written: RefCell<Vec<(String, ProjectChanges)>>,
}

impl Storage for FakeStorage {
    fn stored_hashes(&self, project: &str) -> Result<HashMap<String, u64>, StorageError> {
        Ok(self.stored.get(project).cloned().unwrap_or_default())
    }

    fn write_project(
        &self,
        project: &str,
        changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError> {
        if self.fail_writes {
            return Err(StorageError("connection refused".to_owned()));
        }
        self.written.borrow_mut().push((
            project.to_owned(),
            ProjectChanges {
                upserted: changes.upserted.clone(),
                removed: changes.removed.clone(),
            },
        ));
        Ok(WriteResult {
            relinked: 3,
            stale_annotations: 1,
            orphaned_annotations: 2,
        })
    }

    fn describe(&self) -> String {
        "Storage: fake".to_owned()
    }
}

/// Reports files containing `PARTIAL` as parsed in part.
struct PartialParser;

impl Parser for PartialParser {
    fn parse(&self, path: &Path, contents: &[u8]) -> ParseOutcome {
        if contents.windows(7).any(|window| window == b"PARTIAL") {
            ParseOutcome::Partial("unsupported syntax".to_owned())
        } else {
            TextParser.parse(path, contents)
        }
    }
}

fn all() -> IndexArgs {
    IndexArgs {
        id: None,
        full: false,
        changed: false,
        force: false,
    }
}

fn only(id: &str) -> IndexArgs {
    IndexArgs {
        id: Some(id.to_owned()),
        ..all()
    }
}

fn report<'a>(run: &'a IndexRun, id: &str) -> Option<&'a ProjectReport> {
    run.projects.iter().find(|project| project.id == id)
}

#[test]
fn parses_index_flags() {
    let cli = Cli::try_parse_from(["workbench", "index", "--id", "api", "--full"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Index(IndexArgs { id: Some(id), full: true, changed: false, force: false })) if id == "api"
    ));
    assert!(Cli::try_parse_from(["workbench", "index", "--full", "--changed"]).is_err());
}

#[test]
fn indexes_source_files_and_skips_everything_else() -> CliResult<()> {
    let scratch = Scratch::new("files", TWO_PROJECTS)?;
    scratch.write("api/src/app.py", "print('hi')")?;
    scratch.write("api/src/types.pyi", "x: int")?;
    scratch.write("api/README.md", "not source")?;
    scratch.write("api/node_modules/dep.py", "")?;
    scratch.write("api/.venv/lib.py", "")?;
    scratch.write("api/env/lib.py", "")?;
    scratch.write("api/src/__pycache__/app.py", "")?;
    scratch.write("api/big.py", vec![b'#'; 1024 * 1024 + 1])?;
    scratch.write("api/bad.py", [0xff, 0xfe, 0x00])?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        scratch.0.join("api/src/app.py"),
        scratch.0.join("api/link.py"),
    )?;
    scratch.write("web/src/main.ts", "export {}")?;
    scratch.write("web/src/view.tsx", "export {}")?;
    scratch.write("web/tool.py", "not typescript")?;

    let run = scratch.index(&all())?;

    let api = report(&run, "api").map(|report| (report.counts, report.status));
    let skipped = if cfg!(unix) { 2 } else { 1 };
    assert_eq!(
        api,
        Some((
            Counts {
                parsed: 2,
                skipped,
                failed: 1,
                relinked: 3,
                stale_annotations: 1,
                orphaned_annotations: 2,
                ..Counts::default()
            },
            Status::Indexed
        ))
    );
    assert_eq!(
        report(&run, "web").map(|report| report.counts.parsed),
        Some(2)
    );
    assert_eq!(run.problems(), ["1 file could not be read or parsed"]);
    let log = scratch.log();
    assert!(log.contains("bad.py: could not be parsed: not valid UTF-8 text"));
    Ok(())
}

#[test]
fn force_parses_large_files_but_still_skips_symbolic_links() -> CliResult<()> {
    let scratch = Scratch::new("force", TWO_PROJECTS)?;
    scratch.write("api/big.py", vec![b'#'; 1024 * 1024 + 1])?;
    scratch.write("api/small.py", "x = 1")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        scratch.0.join("api/small.py"),
        scratch.0.join("api/link.py"),
    )?;
    let links = usize::from(cfg!(unix));
    let counts = |run: &IndexRun| {
        report(run, "api").map(|report| (report.counts.parsed, report.counts.skipped))
    };

    let run = scratch.index(&only("api"))?;
    assert_eq!(counts(&run), Some((1, 1 + links)));

    let run = scratch.index(&IndexArgs {
        force: true,
        ..only("api")
    })?;
    assert_eq!(counts(&run), Some((2, links)));
    assert!(Cli::try_parse_from(["workbench", "index", "--force", "--full"]).is_ok());
    Ok(())
}

#[test]
fn fails_with_a_hint_when_there_are_no_projects() -> CliResult<()> {
    let scratch = Scratch::new("empty", "version: 1\nprojects: []\n")?;

    let error = scratch.index(&all());

    assert!(matches!(error, Err(CliErrors::NoProjects)));
    assert!(
        CliErrors::NoProjects
            .to_string()
            .contains("workbench project add <PATH>")
    );
    Ok(())
}

#[test]
fn indexes_only_the_selected_project() -> CliResult<()> {
    let scratch = Scratch::new("select", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;

    let run = scratch.index(&only("web"))?;

    assert_eq!(
        run.projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        ["web"]
    );
    assert!(matches!(
        scratch.index(&only("billing")),
        Err(CliErrors::UnknownProjectId { known, .. }) if known == ["api", "web"]
    ));
    Ok(())
}

#[test]
fn a_held_lock_stops_the_run_and_a_finished_run_releases_it() -> CliResult<()> {
    let scratch = Scratch::new("lock", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;

    scratch.index(&all())?;
    assert!(!scratch.0.join(".workbench/index.lock").exists());

    scratch.write(".workbench/index.lock", "123")?;
    assert!(matches!(
        scratch.index(&all()),
        Err(CliErrors::IndexLocked(_))
    ));
    Ok(())
}

#[test]
fn an_interrupted_run_stops_and_is_a_failure() -> CliResult<()> {
    let scratch = Scratch::new("interrupted", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    scratch.write("web/main.ts", "")?;
    let storage = FakeStorage::default();

    let run = scratch.index_with(&all(), &storage, &TextParser, true)?;

    assert_eq!(
        report(&run, "api").map(|report| report.status),
        Some(Status::Interrupted)
    );
    assert_eq!(
        report(&run, "web").map(|report| report.status),
        Some(Status::NotReached)
    );
    assert!(storage.written.borrow().is_empty());
    assert_eq!(run.problems(), ["the run was interrupted"]);
    assert!(!scratch.0.join(".workbench/index.lock").exists());
    Ok(())
}

#[test]
fn unchanged_files_are_skipped_and_deleted_files_removed() -> CliResult<()> {
    let scratch = Scratch::new("incremental", TWO_PROJECTS)?;
    scratch.write("api/same.py", "same")?;
    scratch.write("api/edited.py", "new")?;
    let stored = HashMap::from([
        ("same.py".to_owned(), content_hash(b"same")),
        ("edited.py".to_owned(), content_hash(b"old")),
        ("gone.py".to_owned(), 1),
    ]);
    let storage = FakeStorage {
        stored: HashMap::from([("api".to_owned(), stored)]),
        ..FakeStorage::default()
    };

    let run = scratch.index_with(&only("api"), &storage, &TextParser, false)?;

    let counts = report(&run, "api").map(|report| report.counts);
    assert_eq!(
        counts.map(|counts| (counts.parsed, counts.unchanged, counts.removed)),
        Some((1, 1, 1))
    );
    let written = storage.written.borrow();
    assert_eq!(written[0].1.removed, ["gone.py"]);
    assert_eq!(
        written[0]
            .1
            .upserted
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["edited.py"]
    );
    drop(written);

    let full = IndexArgs {
        full: true,
        ..only("api")
    };
    let run = scratch.index_with(&full, &storage, &TextParser, false)?;
    let counts = report(&run, "api").map(|report| report.counts);
    assert_eq!(
        counts.map(|counts| (counts.parsed, counts.unchanged, counts.removed)),
        Some((2, 0, 1))
    );
    Ok(())
}

#[test]
fn partial_files_are_not_failures() -> CliResult<()> {
    let scratch = Scratch::new("partial", TWO_PROJECTS)?;
    scratch.write("api/odd.py", "PARTIAL")?;

    let run = scratch.index_with(&only("api"), &FakeStorage::default(), &PartialParser, false)?;

    assert_eq!(
        report(&run, "api").map(|report| report.counts.partial),
        Some(1)
    );
    assert!(run.problems().is_empty());
    Ok(())
}

#[test]
fn a_failed_write_marks_the_project_and_is_logged() -> CliResult<()> {
    let scratch = Scratch::new("write", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;
    let storage = FakeStorage {
        fail_writes: true,
        ..FakeStorage::default()
    };

    let run = scratch.index_with(&only("api"), &storage, &TextParser, false)?;

    assert_eq!(
        report(&run, "api").map(|report| report.status),
        Some(Status::WriteFailed)
    );
    assert_eq!(run.problems(), ["1 project could not be written"]);
    assert!(
        scratch
            .log()
            .contains("project `api`: could not write the index: connection refused")
    );
    Ok(())
}

#[test]
fn projects_that_cannot_be_read_fail_without_stopping_the_run() -> CliResult<()> {
    let config = "version: 1
projects:
  - id: missing
    path: missing
    language: python
  - id: odd
    path: odd
    language: cobol
  - id: web
    path: web
    language: typescript
";
    let scratch = Scratch::new("unreadable", config)?;
    scratch.write("odd/main.cob", "")?;
    scratch.write("web/main.ts", "")?;

    let run = scratch.index(&all())?;

    assert_eq!(
        report(&run, "missing").map(|report| report.status),
        Some(Status::Failed)
    );
    assert_eq!(
        report(&run, "odd").map(|report| report.status),
        Some(Status::Failed)
    );
    assert_eq!(
        report(&run, "web").map(|report| report.status),
        Some(Status::Indexed)
    );
    assert_eq!(run.problems(), ["2 projects could not be indexed"]);
    assert!(scratch.log().contains("unknown language Some(\"cobol\")"));
    Ok(())
}

fn git(directory: &Path, arguments: &[&str]) -> CliResult<()> {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-C",
        ])
        .arg(directory)
        .args(arguments)
        .output()?
        .status;
    assert!(status.success(), "git {arguments:?} failed");
    Ok(())
}

#[test]
fn changed_indexes_only_what_git_reports() -> CliResult<()> {
    let scratch = Scratch::new("changed", TWO_PROJECTS)?;
    let api = scratch.0.join("api");
    scratch.write("api/kept.py", "kept")?;
    scratch.write("api/edited.py", "old")?;
    scratch.write("api/deleted.py", "deleted")?;
    git(&api, &["init", "-q"])?;
    git(&api, &["add", "."])?;
    git(&api, &["commit", "-q", "-m", "initial"])?;
    scratch.write("api/edited.py", "new")?;
    scratch.write("api/added.py", "added")?;
    fs::remove_file(api.join("deleted.py"))?;
    let storage = FakeStorage {
        stored: HashMap::from([(
            "api".to_owned(),
            HashMap::from([("deleted.py".to_owned(), 1), ("kept.py".to_owned(), 2)]),
        )]),
        ..FakeStorage::default()
    };

    let changed = IndexArgs {
        changed: true,
        ..only("api")
    };
    let run = scratch.index_with(&changed, &storage, &TextParser, false)?;

    let counts = report(&run, "api").map(|report| (report.counts.parsed, report.counts.removed));
    assert_eq!(counts, Some((2, 1)));
    let written = storage.written.borrow();
    let mut upserted: Vec<&str> = written[0]
        .1
        .upserted
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    upserted.sort_unstable();
    assert_eq!(upserted, ["added.py", "edited.py"]);
    assert_eq!(written[0].1.removed, ["deleted.py"]);
    Ok(())
}

#[test]
fn changed_outside_a_git_repository_fails_the_project() -> CliResult<()> {
    let scratch = Scratch::new("no-git", TWO_PROJECTS)?;
    scratch.write("api/app.py", "")?;

    let changed = IndexArgs {
        changed: true,
        ..only("api")
    };
    let run = scratch.index(&changed)?;

    assert_eq!(
        report(&run, "api").map(|report| report.status),
        Some(Status::Failed)
    );
    assert!(scratch.log().contains("could not list changed files"));
    Ok(())
}

#[test]
fn summary_is_a_table_of_every_count() {
    let run = IndexRun {
        projects: vec![
            ProjectReport {
                id: "api".to_owned(),
                counts: Counts {
                    parsed: 12,
                    failed: 1,
                    ..Counts::default()
                },
                status: Status::Indexed,
            },
            ProjectReport {
                id: "web".to_owned(),
                counts: Counts::default(),
                status: Status::WriteFailed,
            },
        ],
        interrupted: false,
    };

    assert_eq!(
        run.summary("Storage: fake", false),
        [
            "+---------+--------------+--------+-----------+---------+---------+---------+--------+-----------+-------+----------+",
            "| PROJECT | STATUS       | PARSED | UNCHANGED | REMOVED | SKIPPED | PARTIAL | FAILED | RE-LINKED | STALE | ORPHANED |",
            "+---------+--------------+--------+-----------+---------+---------+---------+--------+-----------+-------+----------+",
            "| api     | indexed      | 12     | 0         | 0       | 0       | 0       | 1      | 0         | 0     | 0        |",
            "| web     | write failed | 0      | 0         | 0       | 0       | 0       | 0      | 0         | 0     | 0        |",
            "+---------+--------------+--------+-----------+---------+---------+---------+--------+-----------+-------+----------+",
            "Storage: fake",
        ]
    );
    assert_eq!(
        run.problems(),
        [
            "1 file could not be read or parsed",
            "1 project could not be written"
        ]
    );
}
