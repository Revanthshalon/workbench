//! `workbench index`: reads the projects' files and writes the index to storage.
//!
//! Progress goes to stderr and the summary to stdout. Project files are only ever
//! read. Details of every failure go to the log in `.workbench/logs/`; the terminal
//! shows counts and a short message.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use clap::Args;
use console::{Style, colors_enabled};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::{
    cli::{
        constants::{MAX_INDEXED_FILE_BYTES, SRC_PACKAGE_FILES},
        errors::{CliErrors, CliResult},
        log::Logger,
        table,
        workspace::{ExistingProject, Workspace},
    },
    graph::{FileGraph, SourceFile},
};

mod collisions;
mod discover;
mod lock;
mod parser;
mod report;
mod storage;
#[cfg(test)]
mod tests;

use collisions::ModuleFile;
use discover::{Filter, index_path};
use lock::IndexLock;
use parser::{ParseOutcome, Parser, TextParser, python::PythonParser};
use report::{Counts, IndexRun, ProjectReport, Status};
use storage::{MemgraphPlaceholder, ProjectChanges, Storage};

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Index only the project with this id. Default: every project.
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,
    /// Re-parse every file, ignoring what the last run stored.
    #[arg(long, conflicts_with = "changed")]
    pub full: bool,
    /// Only index the files git reports as changed or untracked.
    #[arg(long)]
    pub changed: bool,
    /// Parse files of any size. Without it, files over 1 MiB are skipped.
    #[arg(long)]
    pub force: bool,
    /// Write every parsed file's graph (nodes, edges, references) to the log.
    #[arg(long)]
    pub log_graph: bool,
}

/// Set by Ctrl-C; the run stops before the next file.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Indexes the workspace's projects, prints the summary, and fails when any file,
/// project or the run itself did not complete.
pub fn run(args: &IndexArgs) -> CliResult<()> {
    // Without the handler Ctrl-C still stops the run, just without a summary.
    let _handler = ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst));

    let workspace = Workspace::find(&env::current_dir()?)?;
    let storage = MemgraphPlaceholder::new(workspace.storage()?);
    let logger = Logger::new(workspace.root(), "index");
    let context = Context {
        workspace: &workspace,
        storage: &storage,
        python_parser: &PythonParser,
        typescript_parser: &TextParser,
        logger: &logger,
        interrupted: &INTERRUPTED,
        progress: true,
    };

    let run = execute(args, &context)?;
    for line in run.summary(&storage.describe(), colors_enabled()) {
        println!("{line}");
    }
    if args.log_graph {
        println!(
            "{}",
            table::paint(
                &Style::new().dim(),
                &format!(
                    "Graphs of parsed files are in {}",
                    workspace.relative_path(logger.path())
                ),
                colors_enabled(),
            )
        );
    }
    let problems = run.problems();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CliErrors::IndexIncomplete(problems))
    }
}

/// Everything a run works with, so tests can swap in their own storage and parsers.
pub(super) struct Context<'a> {
    pub workspace: &'a Workspace,
    pub storage: &'a dyn Storage,
    pub python_parser: &'a dyn Parser,
    pub typescript_parser: &'a dyn Parser,
    pub logger: &'a Logger,
    pub interrupted: &'a AtomicBool,
    /// Show progress bars and per-project lines on stderr.
    pub progress: bool,
}

/// Indexes the selected projects under the workspace lock.
///
/// Fails only when the run cannot start; problems with files or projects are in the
/// returned report.
pub(super) fn execute(args: &IndexArgs, context: &Context<'_>) -> CliResult<IndexRun> {
    let projects = context.workspace.projects()?;
    if projects.is_empty() {
        return Err(CliErrors::NoProjects);
    }
    let selected: Vec<&ExistingProject> = match &args.id {
        None => projects.iter().collect(),
        Some(id) => {
            let project = projects.iter().find(|project| &project.id == id);
            vec![project.ok_or_else(|| CliErrors::UnknownProjectId {
                id: id.clone(),
                known: projects.iter().map(|project| project.id.clone()).collect(),
            })?]
        }
    };

    let _lock = IndexLock::acquire(&context.workspace.workbench_path())?;
    let mut run = IndexRun {
        projects: Vec::new(),
        interrupted: false,
    };
    for project in selected {
        if run.interrupted {
            run.projects.push(ProjectReport {
                id: project.id.clone(),
                counts: Counts::default(),
                status: Status::NotReached,
            });
            continue;
        }
        let report = index_project(project, args, context);
        run.interrupted = report.status == Status::Interrupted;
        if context.progress {
            eprintln!("{}: {}", report.id, progress_line(&report));
        }
        run.projects.push(report);
    }
    Ok(run)
}

/// Reads, parses and writes one project. Never fails: problems end up in the report.
fn index_project(
    project: &ExistingProject,
    args: &IndexArgs,
    context: &Context<'_>,
) -> ProjectReport {
    let mut report = ProjectReport {
        id: project.id.clone(),
        counts: Counts::default(),
        status: Status::Failed,
    };
    let fail = |message: String| {
        context
            .logger
            .error(&format!("project `{}`: {message}", project.id));
    };

    let project_root = context.workspace.root().join(&project.path);
    if !project_root.is_dir() {
        fail(format!(
            "directory {} does not exist",
            project_root.display()
        ));
        return report;
    }
    let Some(extensions) = project.language.as_deref().and_then(discover::extensions) else {
        fail(format!(
            "unknown language {:?}; expected python or typescript",
            project.language
        ));
        return report;
    };
    let venv = project.venv.as_deref().map(Path::new);
    let filter = Filter { extensions, venv };

    // Which files to look at, and which stored files may have been deleted.
    let (candidates, deleted_candidates) = if args.changed {
        match discover::git_changes(&project_root, &filter) {
            Ok(changes) => (changes.present, Some(changes.deleted)),
            Err(details) => {
                fail(format!("could not list changed files: {details}"));
                return report;
            }
        }
    } else {
        match discover::source_files(&project_root, &filter) {
            Ok(files) => (files, None),
            Err(error) => {
                fail(format!("could not list files: {error:?}"));
                return report;
            }
        }
    };

    // Always read, so files deleted since the last run are removed even with `--full`.
    let stored = match context.storage.stored_hashes(&project.id) {
        Ok(stored) => stored,
        Err(error) => {
            fail(format!("could not read the stored index: {}", error.0));
            report.status = Status::WriteFailed;
            return report;
        }
    };
    let no_hashes = HashMap::new();
    let previous = if args.full { &no_hashes } else { &stored };

    let src_is_package = SRC_PACKAGE_FILES
        .iter()
        .any(|file| project_root.join(file).is_file());
    // Collisions come from every file of the project, so ids do not depend on which
    // files `--changed` re-parses.
    let all_files = if args.changed {
        match discover::source_files(&project_root, &filter) {
            Ok(files) => files,
            Err(error) => {
                fail(format!("could not list files: {error:?}"));
                return report;
            }
        }
    } else {
        candidates.clone()
    };
    let naming = ModuleNaming {
        project: &project.id,
        extensions,
        src_is_package,
    };
    let disambiguated = collision_losers(&naming, &all_files, context.logger);

    let files = ProjectFiles {
        project: &project.id,
        root: &project_root,
        parser: match project.language.as_deref() {
            Some("python") => context.python_parser,
            _ => context.typescript_parser,
        },
        previous,
        any_size: args.force,
        log_graph: args.log_graph,
        src_is_package,
        disambiguated: &disambiguated,
    };
    let progress = progress_bar(context.progress, &project.id, candidates.len());
    let mut changes = ProjectChanges::default();
    for relative in &candidates {
        if context.interrupted.load(Ordering::SeqCst) {
            progress.finish_and_clear();
            report.status = Status::Interrupted;
            return report;
        }
        progress.set_message(index_path(relative));
        let file = index_file(&files, relative, context);
        match file {
            FileOutcome::Unchanged => report.counts.unchanged += 1,
            FileOutcome::Skipped => report.counts.skipped += 1,
            FileOutcome::Failed => report.counts.failed += 1,
            FileOutcome::Parsed(indexed) => {
                report.counts.parsed += 1;
                changes.upserted.push(indexed);
            }
            FileOutcome::Partial(indexed) => {
                report.counts.partial += 1;
                changes.upserted.push(indexed);
            }
        }
        progress.inc(1);
    }
    progress.finish_and_clear();

    changes.removed = removed_files(&stored, &candidates, deleted_candidates.as_deref());
    report.counts.removed = changes.removed.len();

    match context.storage.write_project(&project.id, &changes) {
        Ok(written) => {
            report.counts.relinked = written.relinked;
            report.counts.stale_annotations = written.stale_annotations;
            report.counts.orphaned_annotations = written.orphaned_annotations;
            report.status = Status::Indexed;
        }
        Err(error) => {
            fail(format!("could not write the index: {}", error.0));
            report.status = Status::WriteFailed;
        }
    }
    report
}

/// What happened to one file.
enum FileOutcome {
    Unchanged,
    Skipped,
    Failed,
    Parsed(FileGraph),
    Partial(FileGraph),
}

/// What reading and parsing a project's files needs.
struct ProjectFiles<'a> {
    project: &'a str,
    root: &'a Path,
    parser: &'a dyn Parser,
    /// Hashes of files that are skipped as unchanged when their content still matches.
    previous: &'a HashMap<String, u64>,
    /// Lifts the size limit (`--force`).
    any_size: bool,
    /// Write each parsed file's graph to the log (`--log-graph`).
    log_graph: bool,
    /// Whether the project's `src/` is a package, so `src` is part of module names.
    src_is_package: bool,
    /// Paths of files that lose a module-name collision; their ids get `@<path>`.
    disambiguated: &'a HashSet<String>,
}

/// How a project's files are named as modules.
struct ModuleNaming<'a> {
    project: &'a str,
    extensions: &'a [&'a str],
    src_is_package: bool,
}

/// The paths of the files that lose a module-name collision, logging each collision.
fn collision_losers(
    naming: &ModuleNaming<'_>,
    all_files: &[PathBuf],
    logger: &Logger,
) -> HashSet<String> {
    let modules: Vec<ModuleFile> = all_files
        .iter()
        .map(|relative| {
            let path = index_path(relative);
            let module = SourceFile {
                project: naming.project,
                path: &path,
                src_is_package: naming.src_is_package,
                disambiguator: None,
            }
            .module_name();
            ModuleFile {
                stub: path.ends_with(".pyi"),
                path,
                module,
            }
        })
        .collect();
    let mut losers = HashSet::new();
    for collision in collisions::losers(&modules, naming.extensions, naming.src_is_package) {
        logger.info(&format!(
            "project `{}`: module `{}` is defined by {} and {}; \
             ids from the others get the suffix @<path>",
            naming.project,
            collision.module,
            collision.winner,
            collision.losers.join(", ")
        ));
        losers.extend(collision.losers);
    }
    losers
}

/// Reads and parses one file of a project.
fn index_file(files: &ProjectFiles<'_>, relative: &Path, context: &Context<'_>) -> FileOutcome {
    let path = files.root.join(relative);
    let key = index_path(relative);
    let log = |message: &str| {
        context
            .logger
            .error(&format!("{}: {message}", path.display()));
    };

    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            log(&format!("could not be read: {error:?}"));
            return FileOutcome::Failed;
        }
    };
    // Symbolic links are always skipped; large files only without `--force`.
    let too_large = !files.any_size && metadata.len() > MAX_INDEXED_FILE_BYTES;
    if metadata.file_type().is_symlink() || too_large {
        return FileOutcome::Skipped;
    }
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) => {
            log(&format!("could not be read: {error:?}"));
            return FileOutcome::Failed;
        }
    };

    let file = SourceFile {
        project: files.project,
        path: &key,
        src_is_package: files.src_is_package,
        disambiguator: files.disambiguated.contains(&key).then_some(key.as_str()),
    };
    let hash = file_hash(&contents, &file);
    if files.previous.get(&key) == Some(&hash) {
        return FileOutcome::Unchanged;
    }
    match files.parser.parse(&file, &contents) {
        ParseOutcome::Parsed(mut graph) => {
            graph.hash = hash;
            log_graph(files, &graph, context);
            FileOutcome::Parsed(graph)
        }
        ParseOutcome::Partial(mut graph, details) => {
            log(&format!("parsed in part: {details}"));
            graph.hash = hash;
            log_graph(files, &graph, context);
            FileOutcome::Partial(graph)
        }
        ParseOutcome::Failed(details) => {
            log(&format!("could not be parsed: {details}"));
            FileOutcome::Failed
        }
    }
}

/// With `--log-graph`, writes a parsed file's graph to the log.
fn log_graph(files: &ProjectFiles<'_>, graph: &FileGraph, context: &Context<'_>) {
    if files.log_graph {
        context.logger.info(&format!(
            "graph of `{}` {}\n{}",
            files.project,
            graph.path,
            graph.describe()
        ));
    }
}

/// Stored files that are gone. With `--changed`, only those git reports as deleted.
fn removed_files(
    stored: &HashMap<String, u64>,
    candidates: &[PathBuf],
    deleted: Option<&[PathBuf]>,
) -> Vec<String> {
    let mut removed: Vec<String> = match deleted {
        Some(deleted) => deleted
            .iter()
            .map(|path| index_path(path))
            .filter(|path| stored.contains_key(path))
            .collect(),
        None => {
            let present: HashSet<String> = candidates.iter().map(|path| index_path(path)).collect();
            stored
                .keys()
                .filter(|path| !present.contains(*path))
                .cloned()
                .collect()
        }
    };
    removed.sort();
    removed
}

/// The hash that decides whether a file is unchanged since the last run: its contents,
/// plus how it is named. A file that starts or stops losing a module-name collision,
/// or whose project's `src/` becomes or stops being a package, gets different ids
/// without its contents changing, so it must be parsed again. Files with neither keep
/// the plain content hash.
fn file_hash(contents: &[u8], file: &SourceFile<'_>) -> u64 {
    let mut hash = content_hash(contents);
    if let Some(path) = file.disambiguator {
        hash = fnv1a(fnv1a(hash, b"\0@"), path.as_bytes());
    }
    if file.src_is_package {
        hash = fnv1a(hash, b"\0src-package");
    }
    hash
}

/// FNV-1a: a stable 64-bit hash of a file's contents.
fn content_hash(contents: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    fnv1a(OFFSET, contents)
}

/// Continues an FNV-1a hash over more bytes.
fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

/// A progress bar on stderr, or a hidden one when progress is off.
fn progress_bar(visible: bool, id: &str, files: usize) -> ProgressBar {
    if !visible {
        return ProgressBar::hidden();
    }
    let length = u64::try_from(files).unwrap_or(u64::MAX);
    let bar = ProgressBar::with_draw_target(Some(length), ProgressDrawTarget::stderr());
    if let Ok(style) =
        ProgressStyle::with_template("{prefix:.cyan.bold} [{bar:30}] {pos}/{len} {wide_msg:.dim}")
    {
        bar.set_style(style.progress_chars("=> "));
    }
    bar.set_prefix(id.to_owned());
    bar
}

/// The line printed to stderr when a project is done.
fn progress_line(report: &ProjectReport) -> String {
    let counts = report.counts;
    match report.status {
        Status::Indexed => format!(
            "{} parsed, {} unchanged, {} failed",
            counts.parsed, counts.unchanged, counts.failed
        ),
        Status::WriteFailed => "could not be written".to_owned(),
        Status::Failed => "could not be indexed".to_owned(),
        Status::Interrupted => "interrupted".to_owned(),
        Status::NotReached => "not reached".to_owned(),
    }
}
