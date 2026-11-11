//! `stone code native`: exact edges from the language SDKs' SCIP indexers.
//!
//! Tree-sitter stays the always-on index. This pass reads SCIP indexes
//! (rust-analyzer, scip-typescript, scip-dotnet, scip-python, scip-dart, or any `.scip`
//! file) and stores the call sites they resolve in `native_edges`, next to the
//! tree-sitter `edges`. Reads see a native edge instead of the tree-sitter
//! edges at the same site (the `edge_ends` view), so a tree-sitter refresh
//! never rewrites native data and an incremental refresh still equals a full
//! rebuild of the tree-sitter rows. A replaced file takes its symbols with it,
//! and with them every native edge that starts or ends there; the language is
//! then reported as out of date until the pass runs again.
//!
//! How SCIP maps onto the graph:
//! - A document is a file, matched by path under the index's project root.
//! - A definition occurrence is joined to our declaration of the same
//!   identifier whose line span holds it (the innermost one); that is the only
//!   way a SCIP symbol string becomes one of ours.
//! - A reference occurrence becomes an edge when we hold a site with the same
//!   line and name in that file: the site gives the source symbol and the edge
//!   kind, the definition gives the target. SCIP has columns and we do not, so
//!   two sites of one name on one line cannot be told apart; both point at what
//!   their occurrences resolve to.
//! - A symbol of a package the index defines nothing of (the standard
//!   library, a dependency) is outside the repo: the site stores a native edge
//!   with no target, which hides our inferred and bucket edges there and adds
//!   none. Exact tree-sitter edges at the site stay. A symbol of a package the
//!   index does define, but that we could not join to a declaration, maps to
//!   nothing and the tree-sitter edges stay.
//! - One SCIP symbol can be defined at several places (a partial class); the
//!   edge then goes to each declaration. A definition whose identifier is not
//!   one of our declarations (generated code, a macro) joins nothing.
//! - Columns are counted in the document's position encoding (UTF-8 bytes,
//!   UTF-16 units or characters); the identifier is cut from the file on disk
//!   with that encoding, and a range that no longer fits (the file changed
//!   after the index was made) is dropped, so a stale index adds nothing wrong.
//! - An `is_implementation` relationship between two members becomes a
//!   `dispatch` edge from the base member to the implementer.
//!
//! The indexes a pass read are kept under `.stone/native`, with the content
//! hash of every file they matched. When the code index is rebuilt (a new
//! version of stone, a new index format) the kept indexes are read again, so
//! the native edges come back in seconds instead of a rerun of the indexers.
//! A file changed since its index was made is left out and reported out of
//! date, as after any edit.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use carvestone_graph::{
    Encoding, Lang, ProjectKind, RefKind, ScipIndex, ScipOccurrence, ScipSymbol, parse_scip, scip_symbol, slice_line,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::code::open;
use crate::code_index::{contains, lang_from, meta_value, set_meta};
use crate::repo::{EngineError, Repo};
use crate::repos::Repos;

/// One SDK indexer the pass can run.
struct Indexer {
    name: &'static str,
    /// The executable on `PATH`: the name, except where the install names it otherwise.
    program: &'static str,
    label: &'static str,
    langs: &'static [Lang],
    /// The project files whose directory it runs in.
    manifests: &'static [ProjectKind],
    /// The command that puts it on `PATH`.
    install: &'static str,
}

const INDEXERS: [Indexer; 5] = [
    Indexer {
        name: "rust-analyzer",
        program: "rust-analyzer",
        label: "Rust",
        langs: &[Lang::Rust],
        manifests: &[ProjectKind::Cargo],
        install: "rustup component add rust-analyzer",
    },
    Indexer {
        name: "scip-typescript",
        program: "scip-typescript",
        label: "TypeScript",
        langs: &[Lang::TypeScript, Lang::JavaScript],
        manifests: &[ProjectKind::Npm, ProjectKind::TsConfig],
        install: "npm install -g @sourcegraph/scip-typescript",
    },
    Indexer {
        name: "scip-dotnet",
        program: "scip-dotnet",
        label: "C#",
        langs: &[Lang::CSharp],
        manifests: &[ProjectKind::Csproj],
        install: "dotnet tool install --global scip-dotnet",
    },
    Indexer {
        name: "scip-python",
        program: "scip-python",
        label: "Python",
        langs: &[Lang::Python],
        manifests: &[ProjectKind::PyProject],
        install: "npm install -g @sourcegraph/scip-python",
    },
    // `dart pub global activate` puts `scip_dart` in the pub cache's `bin` folder, which must be
    // on `PATH`. It reads the package config `pub get` writes, so each Dart or Flutter package
    // needs `dart pub get` (or `flutter pub get`) first; stone does not run it.
    Indexer {
        name: "scip-dart",
        program: "scip_dart",
        label: "Dart",
        langs: &[Lang::Dart],
        manifests: &[ProjectKind::Pubspec],
        install: "dart pub global activate scip_dart",
    },
];

/// Where `pub get` leaves the package config scip-dart reads.
const DART_PACKAGE_CONFIG: &str = ".dart_tool/package_config.json";

/// A language of the code roots and the indexer that gives it exact edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LangSetup {
    /// `C#`, `TypeScript` (JavaScript counts with it), `Rust`, `Python`.
    pub label: &'static str,
    pub files: usize,
    pub indexer: &'static str,
    /// Whether the indexer is on the search path now.
    pub installed: bool,
    pub install: &'static str,
}

/// The languages under the code roots, most files first, each with its indexer and whether that
/// is installed. Reads the file list only, so it is cheap before the first index exists.
/// `search_path` stands in for `PATH` (tests use it).
pub fn setup(repo: &Repo, search_path: Option<OsString>) -> Vec<LangSetup> {
    let langs = crate::code_index::code_langs(repo);
    let search = search_path.or_else(|| std::env::var_os("PATH")).unwrap_or_default();
    let mut out: Vec<LangSetup> = INDEXERS
        .iter()
        .map(|indexer| LangSetup {
            label: indexer.label,
            files: langs.iter().filter(|lang| indexer.langs.contains(lang)).count(),
            indexer: indexer.name,
            installed: find_program(indexer.program, &search).is_some(),
            install: indexer.install,
        })
        .filter(|lang| lang.files > 0)
        .collect();
    out.sort_by(|a, b| b.files.cmp(&a.files).then(a.label.cmp(b.label)));
    out
}

#[derive(Debug, Clone, Default)]
pub struct NativeQuery {
    /// `.scip` files to read. With none, the indexers found on `PATH` run and the config's
    /// `scip_indexes` files are read.
    pub indexes: Vec<PathBuf>,
    /// Directories to look for indexers in instead of `PATH` (tests use this).
    pub search_path: Option<OsString>,
    /// How long one indexer may run before it is killed; [`DEFAULT_TIMEOUT`] when unset.
    pub timeout: Option<Duration>,
    /// Run every indexer in every directory, also where the kept index is still current.
    pub all: bool,
    /// Run the indexers only in the directories of this repo (its folder); the others keep the
    /// links of their kept indexes.
    pub repo: Option<String>,
}

/// An indexer that has not finished after this long is stopped.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// How often a running indexer is reported while it works.
const TICK: Duration = Duration::from_secs(5);

/// What happened to one indexer or index file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct IndexerRun {
    pub indexer: String,
    /// `ran`, `read` (an index file given with `--index`), `skipped` or `failed`.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub status: &'static str,
    /// Why it was skipped or failed; empty otherwise.
    pub detail: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub millis: u64,
}

/// One step of a native pass, as it happens, for a progress display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum NativeStep {
    /// An indexer started in `dir` (`.` is the root): run `run` of the `runs` it needs.
    Started {
        indexer: String,
        dir: String,
        run: usize,
        runs: usize,
    },
    /// It is still working: seconds so far and the last line it printed.
    Running {
        indexer: String,
        dir: String,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        secs: u64,
        last: String,
    },
    /// An indexer ran, was read, skipped or failed.
    Finished { run: IndexerRun },
    /// Every index is in; the edges are being stored.
    Storing,
}

/// A native pass as the web UI follows it: `GET /api/code/native` and `event: native`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NativeStatus {
    pub running: bool,
    /// The steps of the current or last pass; a running indexer keeps only its latest tick.
    pub steps: Vec<NativeStep>,
    /// The result of the last pass that finished.
    pub report: Option<NativeReport>,
    /// Why the last pass stopped, when it failed.
    pub error: Option<String>,
}

impl NativeStatus {
    /// A new pass begins: the steps of the last one go.
    pub fn start(&mut self) {
        *self = NativeStatus {
            running: true,
            ..NativeStatus::default()
        };
    }

    pub fn push(&mut self, step: NativeStep) {
        if matches!(step, NativeStep::Running { .. }) && matches!(self.steps.last(), Some(NativeStep::Running { .. })) {
            self.steps.pop();
        }
        self.steps.push(step);
    }

    pub fn finish(&mut self, result: Result<NativeReport, String>) {
        self.running = false;
        match result {
            Ok(report) => self.report = Some(report),
            Err(error) => self.error = Some(error),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NativeReport {
    pub indexers: Vec<IndexerRun>,
    /// Documents of the indexes that matched a file of the code graph.
    pub documents: usize,
    /// Native edges stored now, over all runs.
    pub edges: usize,
    /// Inferred edges (a name guess or a dispatch) that a native edge now makes exact. The three
    /// counts take one scan of the whole graph, which costs most of a quick pass on a large one,
    /// so they are counted only when no kept index was reused; `None` otherwise.
    pub upgraded: Option<usize>,
    /// Inferred edges at sites the indexer resolved outside the repo: hidden, nothing replaces them.
    pub hidden: Option<usize>,
    /// Tree-sitter edges that were exact already and agree with a native edge.
    pub confirmed: Option<usize>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub millis: u64,
}

// ---------------------------------------------------------------- running

/// `program` as a file in one of the directories of `search`, trying the
/// executable extensions on Windows.
fn find_program(program: &str, search: &OsString) -> Option<PathBuf> {
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    for dir in std::env::split_paths(search) {
        let plain = dir.join(program);
        if plain.is_file() && extensions.is_empty() {
            return Some(plain);
        }
        for ext in &extensions {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The repo folder holding `path`, `None` for a path in no repo.
fn owner_of<'a>(repos: &'a Repos, path: &str) -> Option<&'a str> {
    repos.repo_of(path).map(|(one, _)| one.dir())
}

/// The directories an indexer runs in, root-relative (`""` is the product root): the top-most
/// ones holding one of its project files within one repo, so a run never spans two repos. When
/// no project file is known, one run per repo that holds files of its languages (the root when
/// those files are in no repo or in the root repo).
fn run_dirs(repo: &Repo, conn: &Connection, indexer: &Indexer) -> Result<Vec<String>, EngineError> {
    let repos = repo.repos();
    let mut stmt = conn.prepare("SELECT path FROM files WHERE lang = 'project'")?;
    let paths: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
    let mut dirs: Vec<String> = paths
        .iter()
        .filter(|path| ProjectKind::of(path).is_some_and(|kind| indexer.manifests.contains(&kind)))
        .map(|path| path.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_string()))
        .collect();
    dirs.sort_by_key(|dir| (dir.matches('/').count() + usize::from(!dir.is_empty()), dir.clone()));
    dirs.dedup();
    let mut top: Vec<String> = Vec::new();
    for dir in dirs {
        if !top
            .iter()
            .any(|kept| contains(kept, &dir) && owner_of(&repos, kept) == owner_of(&repos, &dir))
        {
            top.push(dir);
        }
    }
    if top.is_empty() {
        let mut stmt = conn.prepare("SELECT path, lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
        let files: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut owners: BTreeSet<String> = BTreeSet::new();
        for (path, lang) in files {
            if indexer.langs.iter().any(|l| l.as_str() == lang) {
                owners.insert(owner_of(&repos, &path).unwrap_or_default().to_string());
            }
        }
        top.extend(owners);
    }
    if top.is_empty() {
        top.push(String::new());
    }
    Ok(top)
}

/// scip-python as released stops at start-up on Windows: it builds a pattern from the path
/// separator, and `\` alone is no pattern. It also lists packages with pip, which fails there.
/// So on Windows stone starts its script under node with a preload that escapes that pattern,
/// and hands it an empty package list: links into the repo's own code need no packages.
/// The command and the package list to pass it; `None` elsewhere, or when the script is not
/// beside the launcher as npm installs it.
fn windows_python(indexer: &Indexer, program: &Path, out: &Path) -> Result<Option<(Command, PathBuf)>, String> {
    const PRELOAD: &str = "const R = RegExp;\nconst fix = (p) => (p === \"\\\\\" ? \"\\\\\\\\\" : p);\nglobalThis.RegExp = new Proxy(R, {\n  construct: (t, [p, f]) => new t(fix(p), f),\n  apply: (t, _, [p, f]) => t(fix(p), f),\n});\n";
    if !cfg!(windows) || indexer.name != "scip-python" {
        return Ok(None);
    }
    let Some(script) = program
        .parent()
        .map(|dir| dir.join("node_modules/@sourcegraph/scip-python/index.js"))
        .filter(|script| script.is_file())
    else {
        return Ok(None);
    };
    let preload = out.with_file_name("scip-python-preload.cjs");
    let packages = out.with_file_name("scip-python-packages.json");
    std::fs::write(&preload, PRELOAD)
        .and_then(|()| std::fs::write(&packages, "[]"))
        .map_err(|err| format!("could not write its start-up files: {err}"))?;
    let mut cmd = Command::new("node");
    cmd.arg("-r").arg(&preload).arg(&script);
    Ok(Some((cmd, packages)))
}

/// Runs `indexer` in `dir` and writes its index to `out`. The tool is killed
/// when it has not finished by `timeout`; every [`TICK`] meanwhile, `tick` hears
/// the seconds so far and the last line it printed. The error is that last
/// line, or why it could not start or finish.
fn run_indexer(
    indexer: &Indexer,
    program: &Path,
    dir: &Path,
    out: &Path,
    timeout: Duration,
    tick: &mut dyn FnMut(u64, &str),
) -> Result<(), String> {
    if indexer.name == "scip-dart" && !dir.join(DART_PACKAGE_CONFIG).is_file() {
        return Err("no package config; run `dart pub get` (or `flutter pub get`) in this folder first".to_string());
    }
    let (mut cmd, packages) = match windows_python(indexer, program, out)? {
        Some((cmd, packages)) => (cmd, Some(packages)),
        None => (Command::new(program), None),
    };
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // Flags as the tools document them; each indexes its working directory, which is `dir`.
    match indexer.name {
        "rust-analyzer" => cmd.args(["scip", ".", "--exclude-vendored-libraries", "--output"]),
        // A JavaScript project has no tsconfig.json; this makes the tool write one in memory.
        "scip-typescript" => cmd.args(["index", "--infer-tsconfig", "--output"]),
        // Without a project name the symbols carry no package, so the pass could not tell
        // the repo's own code from a dependency.
        "scip-python" => {
            let name = dir
                .file_name()
                .map_or_else(|| "project".to_string(), |name| name.to_string_lossy().into_owned());
            cmd.args(["index", "--project-name", &name]);
            if let Some(packages) = &packages {
                cmd.arg("--environment").arg(packages);
            }
            cmd.arg("--output")
        }
        // It indexes its working directory and always writes `index.scip` there; moved to `out`
        // below. No path argument: a relative one (`./`) makes it miss the package on Windows.
        "scip-dart" => &mut cmd,
        _ => cmd.args(["index", "--output"]),
    };
    if indexer.name != "scip-dart" {
        cmd.arg(out);
    }
    let mut child = cmd.spawn().map_err(|err| format!("could not start: {err}"))?;

    // Only the last telling line of stderr is kept, so a chatty tool cannot fill memory.
    let last = Arc::new(Mutex::new(String::new()));
    let (done, finished) = mpsc::channel();
    if let Some(stderr) = child.stderr.take() {
        let last = Arc::clone(&last);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
                let text = String::from_utf8_lossy(&line);
                if telling(&text)
                    && let Ok(mut held) = last.lock()
                {
                    *held = text.trim().to_string();
                }
                line.clear();
            }
            let _ = done.send(());
        });
    }

    let begun = Instant::now();
    let deadline = begun + timeout;
    let mut next_tick = begun + TICK;
    let status = loop {
        if Instant::now() >= next_tick {
            next_tick += TICK;
            let held = last.lock().map(|held| held.clone()).unwrap_or_default();
            tick(begun.elapsed().as_secs(), &held);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "timed out after {}s; raise it with --timeout <secs>",
                    timeout.as_secs().max(1)
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(err) => return Err(format!("could not wait for it: {err}")),
        }
    };
    // A helper the tool left running can hold the pipe open; do not wait on it for long.
    let _ = finished.recv_timeout(Duration::from_millis(500));
    // Its pub launcher exits 0 after a crash, so only the index it leaves tells success.
    if indexer.name == "scip-dart" && status.success() && dir.join("index.scip").is_file() {
        std::fs::rename(dir.join("index.scip"), out).map_err(|err| format!("could not keep its index: {err}"))?;
    }
    if status.success() && out.is_file() {
        return Ok(());
    }
    let last = last.lock().map(|held| held.clone()).unwrap_or_default();
    Err(if last.is_empty() {
        format!("exited with {status}")
    } else {
        last
    })
}

/// A stderr line worth reporting: not blank, not a stack frame (Node's `at …`, Dart's `#3 …` and
/// `<asynchronous suspension>`), and not the `Node.js v24.1.0` trailer a Node tool prints after
/// its error when it crashes.
fn telling(line: &str) -> bool {
    let line = line.trim();
    let dart_frame = line
        .strip_prefix('#')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    !line.is_empty()
        && !line.starts_with("at ")
        && !line.starts_with("Node.js v")
        && !dart_frame
        && line != "<asynchronous suspension>"
}

fn dir_name(dir: &str) -> &str {
    if dir.is_empty() { "." } else { dir }
}

/// Reads the given indexes, or runs the indexers found on `PATH` for the
/// languages the repo holds, and stores the edges they resolve. Running it
/// twice stores the same edges.
pub fn native(repo: &Repo, q: &NativeQuery) -> Result<NativeReport, EngineError> {
    native_with(repo, q, &mut |_| {})
}

/// Adds `run` to the report and tells `progress`.
fn record(report: &mut NativeReport, progress: &mut dyn FnMut(NativeStep), run: IndexerRun) {
    progress(NativeStep::Finished { run: run.clone() });
    report.indexers.push(run);
}

/// Reads one `.scip` file as a source of this pass. `shown` is the path the report names. The
/// file stays where it is; a copy is kept.
fn read_index(
    file: &Path,
    shown: &str,
    sources: &mut Vec<Source>,
    fresh: &mut Vec<Fresh>,
    report: &mut NativeReport,
    progress: &mut dyn FnMut(NativeStep),
) -> Result<(), EngineError> {
    let begun = Instant::now();
    let bytes = std::fs::read(file)
        .map_err(|err| EngineError::Invalid(format!("cannot read {}: {err}; check the path", file.display())))?;
    let parsed = parse_scip(&bytes).map_err(|err| EngineError::Invalid(format!("{}: {err}", file.display())))?;
    let name = if parsed.tool.is_empty() {
        "scip".to_string()
    } else {
        parsed.tool.clone()
    };
    fresh.push(Fresh {
        at: file.to_path_buf(),
        source: sources.len(),
        copy: true,
    });
    sources.push(Source {
        parsed,
        hint: String::new(),
        indexer: name.clone(),
    });
    let run = IndexerRun {
        indexer: format!("{name} ({shown})"),
        status: "read",
        detail: String::new(),
        millis: millis(begun),
    };
    record(report, progress, run);
    Ok(())
}

/// [`native`], telling `progress` each step as it happens.
pub fn native_with(
    repo: &Repo,
    q: &NativeQuery,
    progress: &mut dyn FnMut(NativeStep),
) -> Result<NativeReport, EngineError> {
    let started = Instant::now();
    let index = open(repo)?;
    let conn = index.conn();
    let root = &repo.store.layout.root;
    let mut report = NativeReport::default();
    // Every index of the pass is read before any is stored, so a call from one
    // project into a sibling that another run indexed is seen as inside the repo.
    let mut sources: Vec<Source> = Vec::new();
    let mut fresh: Vec<Fresh> = Vec::new();
    // Files a kept index has, but in an older version: left out, so they read as out of date.
    let mut dropped: BTreeSet<i64> = BTreeSet::new();
    let scratch = root.join(".stone").join("native");

    if !q.indexes.is_empty() {
        for file in &q.indexes {
            read_index(
                file,
                &file.display().to_string(),
                &mut sources,
                &mut fresh,
                &mut report,
                progress,
            )?;
        }
    } else {
        let present: BTreeSet<String> = {
            let mut stmt =
                conn.prepare("SELECT DISTINCT lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
            stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?
        };
        let search = q
            .search_path
            .clone()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let timeout = q.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let repos = repo.repos();
        if let Some(name) = &q.repo
            && !repos.all().iter().any(|one| one.dir() == name)
        {
            let names: Vec<&str> = repos
                .all()
                .iter()
                .map(|one| one.dir())
                .filter(|dir| !dir.is_empty())
                .collect();
            return Err(EngineError::Invalid(if names.is_empty() {
                format!("no repo {name} in this product; run `stone code native` without --repo")
            } else {
                format!("no repo {name} in this product; use one of: {}", names.join(", "))
            }));
        }
        std::fs::create_dir_all(&scratch)
            .map_err(|err| EngineError::Invalid(format!("cannot write .stone/native: {err}")))?;
        let _ = std::fs::write(scratch.join(RUNNING), std::process::id().to_string());
        let _running = Running(scratch.join(RUNNING));
        let kept = read_kept(&scratch);
        let files = code_files(conn)?;
        let changed = changed_files(conn)?;
        for indexer in &INDEXERS {
            let skip = |detail: String| IndexerRun {
                indexer: indexer.name.to_string(),
                status: "skipped",
                detail,
                millis: 0,
            };
            if !indexer.langs.iter().any(|lang| present.contains(lang.as_str())) {
                record(&mut report, progress, skip(format!("no {} files", indexer.label)));
                continue;
            }
            let Some(program) = find_program(indexer.program, &search) else {
                let detail = format!("not on PATH; install it with `{}` to get exact edges", indexer.install);
                record(&mut report, progress, skip(detail));
                continue;
            };
            let begun = Instant::now();
            let mut failures: Vec<String> = Vec::new();
            // A failed run is never kept, so it never replaces a good index.
            let mut ran: Vec<Fresh> = Vec::new();
            std::fs::create_dir_all(&scratch)
                .map_err(|err| EngineError::Invalid(format!("cannot write .stone/native: {err}")))?;
            // A directory whose kept index is still current is read from it instead of run again.
            let mut dirs: Vec<String> = Vec::new();
            let mut reused = 0;
            for dir in run_dirs(repo, conn, indexer)? {
                let wanted = match &q.repo {
                    Some(name) => owner_of(&repos, &dir).unwrap_or_default() == name,
                    None => {
                        q.all
                            || changed.iter().any(|(path, lang)| {
                                indexer.langs.iter().any(|l| l.as_str() == lang)
                                    && (dir.is_empty() || contains(&dir, path))
                            })
                    }
                };
                let entry = kept
                    .indexes
                    .iter()
                    .find(|entry| entry.indexer == indexer.name && entry.hint == dir);
                match entry
                    .filter(|_| !wanted)
                    .and_then(|entry| load_kept(&scratch, root, entry, &files, &mut dropped))
                {
                    Some(source) => {
                        sources.push(source);
                        reused += 1;
                    }
                    // Outside the repo asked for, nothing is run: the links stored there stay.
                    None if q.repo.is_some() && !wanted => {}
                    None => dirs.push(dir),
                }
            }
            if reused > 0 {
                let noun = if reused == 1 { "directory" } else { "directories" };
                let run = IndexerRun {
                    indexer: indexer.name.to_string(),
                    status: "kept",
                    detail: format!("{reused} {noun} unchanged since the last pass"),
                    millis: 0,
                };
                record(&mut report, progress, run);
            }
            if dirs.is_empty() {
                continue;
            }
            for (n, dir) in dirs.iter().enumerate() {
                progress(NativeStep::Started {
                    indexer: indexer.name.to_string(),
                    dir: dir_name(dir).to_string(),
                    run: n + 1,
                    runs: dirs.len(),
                });
                // Named by the process, so two passes started at once never write one file.
                let out = scratch.join(format!("{}-{n}-{}.run.scip", indexer.name, std::process::id()));
                let mut tick = |secs: u64, last: &str| {
                    progress(NativeStep::Running {
                        indexer: indexer.name.to_string(),
                        dir: dir_name(dir).to_string(),
                        secs,
                        last: last.to_string(),
                    });
                };
                let result = run_indexer(indexer, &program, &root.join(dir), &out, timeout, &mut tick).and_then(|()| {
                    let bytes = std::fs::read(&out).map_err(|err| format!("cannot read its index: {err}"))?;
                    parse_scip(&bytes)
                });
                match result {
                    Ok(parsed) => {
                        ran.push(Fresh {
                            at: out,
                            source: sources.len(),
                            copy: false,
                        });
                        sources.push(Source {
                            parsed,
                            hint: dir.clone(),
                            indexer: indexer.name.to_string(),
                        });
                    }
                    // The other directories still run: one failing repo never stops the rest.
                    Err(err) => {
                        let _ = std::fs::remove_file(&out);
                        failures.push(format!("{} in {}: {err}", indexer.name, dir_name(dir)));
                    }
                }
            }
            // Each directory that succeeded is kept on its own; a failed one keeps its earlier index.
            let succeeded = ran.len();
            fresh.append(&mut ran);
            if failures.is_empty() || succeeded > 0 {
                let detail = if failures.is_empty() {
                    String::new()
                } else {
                    format!("{succeeded} of {} directories", dirs.len())
                };
                let run = IndexerRun {
                    indexer: indexer.name.to_string(),
                    status: "ran",
                    detail,
                    millis: millis(begun),
                };
                record(&mut report, progress, run);
            }
            for detail in failures {
                let run = IndexerRun {
                    indexer: indexer.name.to_string(),
                    status: "failed",
                    detail,
                    millis: millis(begun),
                };
                record(&mut report, progress, run);
            }
        }
        // Index files the project lists. The user or CI builds them; stone only reads them.
        for entry in &repo.store.config.scip_indexes {
            let file = root.join(&entry.file);
            if !file.is_file() {
                let run = IndexerRun {
                    indexer: entry.file.clone(),
                    status: "skipped",
                    detail: format!("{} not found; run your indexer first", entry.file),
                    millis: 0,
                };
                record(&mut report, progress, run);
                continue;
            }
            read_index(&file, &entry.file, &mut sources, &mut fresh, &mut report, progress)?;
        }
    }

    progress(NativeStep::Storing);
    let ingested = ingest(conn, root, &sources, &scip_only_langs(repo));
    if ingested.is_ok() {
        forget_covered(conn, &dropped)?;
        keep(conn, root, &scratch, &sources, &fresh)?;
    }
    for run in fresh.iter().filter(|run| !run.copy) {
        let _ = std::fs::remove_file(&run.at);
    }
    report.documents = ingested?;
    report.edges = count(conn, "SELECT count(*) FROM native_edges")?;
    // One scan of the view, and only on a full pass: on a large graph it costs close to a minute.
    if report.indexers.iter().all(|run| run.status != "kept") {
        let (upgraded, hidden, confirmed): (i64, i64, i64) = conn.query_row(
            "SELECT ifnull(sum(exact = 0 AND NOT hidden), 0), ifnull(sum(exact = 0 AND hidden), 0), \
             ifnull(sum(exact = 1), 0) FROM masked_edges",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        report.upgraded = usize::try_from(upgraded).ok();
        report.hidden = usize::try_from(hidden).ok();
        report.confirmed = usize::try_from(confirmed).ok();
    }
    report.millis = millis(started);
    Ok(report)
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn count(conn: &Connection, sql: &str) -> Result<usize, EngineError> {
    let n: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    Ok(usize::try_from(n).unwrap_or(0))
}

// ---------------------------------------------------------------- mapping

/// A declaration of ours a SCIP symbol was defined as.
#[derive(Debug, Clone)]
struct Def {
    /// The index (position in `sources`) whose document defines it.
    source: usize,
    decl: i64,
    file: i64,
    kind: String,
    name: String,
    line: i64,
}

struct DeclRow {
    id: i64,
    name: String,
    kind: String,
    start: i64,
    end: i64,
}

/// A reference occurrence waiting for the definitions of every document.
struct Pending {
    file: i64,
    line: i64,
    ident: String,
    symbol: String,
}

/// One native edge as stored. `file` is the file of the site (of the
/// implementer for a dispatch edge).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Row {
    src: i64,
    /// `None` for a site the index resolved outside the repo.
    dst: Option<i64>,
    kind: String,
    name: String,
    line: i64,
}

/// `file:///c:/repo/sub` as a repo-relative directory (`""` for the root), when it lies inside `root`.
fn base_of(uri: &str, root: &Path) -> Option<String> {
    let mut path = uri.strip_prefix("file://")?.to_string();
    path = percent_decode(&path);
    // `/C:/repo` on Windows.
    if path.len() > 2 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    let norm = |text: &str| -> String {
        let text = text.replace('\\', "/");
        text.strip_prefix("//?/")
            .unwrap_or(&text)
            .trim_end_matches('/')
            .to_string()
    };
    let (path, root) = (norm(&path), norm(&root.to_string_lossy()));
    // Windows paths differ in case only; the rest of the path keeps the case it has on disk.
    let same = |a: &str, b: &str| {
        if cfg!(windows) {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    };
    if same(&path, &root) {
        return Some(String::new());
    }
    let head = path.get(..root.len())?;
    let rest = path[root.len()..].strip_prefix('/')?;
    same(head, &root).then(|| rest.to_string())
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(hex) = text.get(at + 1..at + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn load_decls(conn: &Connection, file: i64) -> Result<Vec<DeclRow>, EngineError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, kind, line_start, line_end FROM symbols WHERE file_id = ?1 AND kind NOT IN ('file', 'project')",
    )?;
    Ok(stmt
        .query_map([file], |row| {
            Ok(DeclRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                start: row.get(3)?,
                end: row.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

/// Reference sites by `(line, name)`: the `(source symbol, kind)` of each.
type SiteMap = HashMap<(i64, String), Vec<(Option<i64>, String)>>;

/// The reference sites of one file.
struct FileSites {
    by_site: SiteMap,
    file_symbol: Option<i64>,
}

fn load_sites(conn: &Connection, file: i64) -> Result<FileSites, EngineError> {
    let mut stmt = conn
        .prepare_cached("SELECT line, name, src_symbol_id, kind FROM refs WHERE file_id = ?1 AND kind <> 'imports'")?;
    let mut by_site = SiteMap::new();
    for row in stmt.query_map([file], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        let (line, name, src, kind) = row?;
        by_site.entry((line, name)).or_default().push((src, kind));
    }
    let file_symbol = conn
        .query_row(
            "SELECT min(id) FROM symbols WHERE file_id = ?1 AND kind = 'file'",
            [file],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    Ok(FileSites { by_site, file_symbol })
}

/// The declaration named `ident` whose lines hold `line`: the innermost, and
/// none when two tie.
fn declared<'a>(decls: &'a [DeclRow], ident: &str, line: i64) -> Option<&'a DeclRow> {
    let mut best: Option<(&DeclRow, bool)> = None;
    for decl in decls
        .iter()
        .filter(|d| d.name == ident && d.start <= line && line <= d.end)
    {
        best = match best {
            Some((old, _)) if old.end - old.start < decl.end - decl.start => best,
            Some((old, _)) if old.end - old.start == decl.end - decl.start => Some((old, true)),
            _ => Some((decl, false)),
        };
    }
    best.filter(|(_, tied)| !tied).map(|(decl, _)| decl)
}

/// `scheme manager name version`: the package a SCIP symbol belongs to.
fn package_of(symbol: &str) -> &str {
    let mut spaces = 0;
    for (at, ch) in symbol.char_indices() {
        if ch == ' ' {
            spaces += 1;
            if spaces == 4 {
                return &symbol[..at];
            }
        }
    }
    symbol
}

/// The name part of a SCIP symbol's package, as a project name would be
/// written: lowercase, `-` and `_` alike (cargo spells a crate both ways).
fn package_name(symbol: &str) -> String {
    symbol
        .split(' ')
        .nth(2)
        .unwrap_or_default()
        .to_lowercase()
        .replace('-', "_")
}

/// One index read in this pass. `hint` is the directory the indexer ran in,
/// used when the index does not say where its project root is.
struct Source {
    parsed: ScipIndex,
    hint: String,
    indexer: String,
}

/// The repo-relative directory the documents of `source` are relative to (`""` for the root).
fn source_base(source: &Source, root: &Path) -> String {
    base_of(&source.parsed.project_root, root).unwrap_or_else(|| source.hint.clone())
}

/// A document's path in the repo.
fn doc_path(base: &str, path: &str) -> String {
    if base.is_empty() {
        path.to_string()
    } else {
        format!("{base}/{path}")
    }
}

/// What matched in one index: the files its documents cover and the reference
/// occurrences waiting for the definitions of every index.
struct Matched {
    base: String,
    pending: Vec<Pending>,
    covered: BTreeSet<i64>,
    langs: BTreeSet<String>,
}

/// The languages only `scip_indexes` lists. A file of one has no tree-sitter symbols, so its
/// symbols and reference sites come from the index.
fn scip_only_langs(repo: &Repo) -> BTreeSet<String> {
    repo.store
        .config
        .scip_indexes
        .iter()
        .map(|entry| entry.language.clone())
        .filter(|language| lang_from(language).is_none())
        .collect()
}

/// The identifier an occurrence covers. A generic call's range can take in its type arguments;
/// the site is named without them.
fn identifier<'a>(lines: &[&'a str], occ: &ScipOccurrence, encoding: Encoding) -> Option<&'a str> {
    let ident = lines
        .get(occ.line as usize)
        .and_then(|line| slice_line(line, occ.start, occ.end, encoding))?;
    Some(ident.split('<').next().unwrap_or(ident).trim_end())
}

/// A symbol as stored: qualified name, name, kind, lines and the qualified name of its parent.
type SymbolKey = (String, String, String, i64, i64, Option<String>);

/// A reference site as stored: line, name, kind and the qualified name and first line of the
/// symbol it sits in (none for a site outside any definition).
type SiteKey = (i64, String, String, Option<(String, i64)>);

/// Makes the symbols (all but the file symbol) and the reference sites of each SCIP-only file the
/// sources cover match its document: definitions give kind, name and lines, nested by qualified
/// name; every other occurrence gives a `references` site inside the innermost definition that
/// holds its line. A file whose rows already match is left alone, so the same index again changes
/// nothing, and any change moves `graph_version`. One transaction.
fn create_symbols(
    conn: &Connection,
    root: &Path,
    sources: &[Source],
    files: &HashMap<String, (i64, String)>,
    scip_langs: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if scip_langs.is_empty() {
        return Ok(());
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut changed = false;
    for source in sources {
        let base = source_base(source, root);
        for doc in &source.parsed.documents {
            let path = doc_path(&base, &doc.path);
            let Some((file, lang)) = files.get(&path) else {
                continue;
            };
            if !scip_langs.contains(lang) {
                continue;
            }
            let Ok(bytes) = std::fs::read(root.join(&path)) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();

            // What the index says: each definition with its lines, then the sites.
            let mut symbols: Vec<(ScipSymbol, i64, i64)> = Vec::new();
            for occ in doc.occurrences.iter().filter(|occ| occ.definition && !occ.import) {
                let Some(symbol) = scip_symbol(&occ.symbol) else {
                    continue;
                };
                let start = i64::from(occ.line) + 1;
                let end = occ.end_line.map_or(start, |line| i64::from(line) + 1).max(start);
                symbols.push((symbol, start, end));
            }
            let mut by_qname: HashMap<&str, usize> = HashMap::new();
            for (at, (symbol, _, _)) in symbols.iter().enumerate() {
                by_qname.entry(symbol.qname.as_str()).or_insert(at);
            }
            let parent_of =
                |symbol: &ScipSymbol| symbol.parent.as_deref().and_then(|qname| by_qname.get(qname).copied());
            let mut sites: Vec<(i64, String, Option<usize>)> = Vec::new();
            for occ in doc.occurrences.iter().filter(|occ| !occ.definition && !occ.import) {
                let Some(ident) = identifier(&lines, occ, doc.encoding) else {
                    continue;
                };
                let line = i64::from(occ.line) + 1;
                // The innermost definition whose lines hold the site; the first of equal spans.
                let src = symbols
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, start, end))| *start <= line && line <= *end)
                    .min_by_key(|(_, (_, start, end))| end - start)
                    .map(|(at, _)| at);
                sites.push((line, ident.to_string(), src));
            }

            let mut want_symbols: Vec<SymbolKey> = symbols
                .iter()
                .map(|(symbol, start, end)| {
                    (
                        symbol.qname.clone(),
                        symbol.name.clone(),
                        symbol.kind.to_string(),
                        *start,
                        *end,
                        parent_of(symbol).map(|at| symbols[at].0.qname.clone()),
                    )
                })
                .collect();
            let mut want_sites: Vec<SiteKey> = sites
                .iter()
                .map(|(line, name, src)| {
                    (
                        *line,
                        name.clone(),
                        RefKind::References.as_str().to_string(),
                        src.map(|at| (symbols[at].0.qname.clone(), symbols[at].1)),
                    )
                })
                .collect();
            want_symbols.sort();
            want_sites.sort();

            // What is stored.
            let mut have_symbols: Vec<SymbolKey> = {
                let mut stmt = tx.prepare_cached(
                    "SELECT s.qname, s.name, s.kind, s.line_start, s.line_end, p.qname FROM symbols s \
                     LEFT JOIN symbols p ON p.id = s.parent_id WHERE s.file_id = ?1 AND s.kind <> 'file'",
                )?;
                let rows = stmt.query_map([file], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })?;
                rows.collect::<Result<_, _>>()?
            };
            let mut have_sites: Vec<SiteKey> = {
                let mut stmt = tx.prepare_cached(
                    "SELECT r.line, r.name, r.kind, s.qname, s.line_start FROM refs r \
                     LEFT JOIN symbols s ON s.id = r.src_symbol_id WHERE r.file_id = ?1",
                )?;
                let rows = stmt.query_map([file], |row| {
                    let qname: Option<String> = row.get(3)?;
                    let start: Option<i64> = row.get(4)?;
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, qname.zip(start)))
                })?;
                rows.collect::<Result<_, _>>()?
            };
            have_symbols.sort();
            have_sites.sort();
            if have_symbols == want_symbols && have_sites == want_sites {
                continue;
            }

            tx.execute("DELETE FROM refs WHERE file_id = ?1", [file])?;
            tx.execute(
                "DELETE FROM symbols_fts WHERE rowid IN (SELECT id FROM symbols WHERE file_id = ?1 AND kind <> 'file')",
                [file],
            )?;
            tx.execute("DELETE FROM symbols WHERE file_id = ?1 AND kind <> 'file'", [file])?;
            let mut insert = tx.prepare_cached(
                "INSERT INTO symbols(file_id, qname, name, kind, parent_id, signature, doc, line_start, line_end, lang, project) \
                 VALUES (?1, ?2, ?3, ?4, NULL, '', NULL, ?5, ?6, ?7, NULL)",
            )?;
            let mut fts = tx.prepare_cached(
                "INSERT INTO symbols_fts(rowid, name, qname, signature, doc) VALUES (?1, ?2, ?3, '', NULL)",
            )?;
            let mut ids: Vec<i64> = Vec::with_capacity(symbols.len());
            for (symbol, start, end) in &symbols {
                insert.execute(params![file, symbol.qname, symbol.name, symbol.kind, start, end, lang])?;
                let id = tx.last_insert_rowid();
                fts.execute(params![id, symbol.name, symbol.qname])?;
                ids.push(id);
            }
            for (at, (symbol, _, _)) in symbols.iter().enumerate() {
                if let Some(parent) = parent_of(symbol) {
                    tx.execute(
                        "UPDATE symbols SET parent_id = ?1 WHERE id = ?2",
                        params![ids[parent], ids[at]],
                    )?;
                }
            }
            let mut site = tx.prepare_cached(
                "INSERT INTO refs(file_id, src_symbol_id, name, qualifier, kind, line) VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            )?;
            for (line, name, src) in &sites {
                let src = src.map(|at| ids[at]);
                site.execute(params![file, src, name, RefKind::References.as_str(), line])?;
            }
            changed = true;
        }
    }
    if changed {
        let version: i64 = meta_value(&tx, "graph_version")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        set_meta(&tx, "graph_version", &(version + 1).to_string())?;
    }
    tx.commit()?;
    Ok(())
}

/// Stores what `sources` resolve; returns how many of their documents matched a
/// file of the code graph. All sources are matched before any edge is decided,
/// so a symbol one run defines is known to the other runs that call into it.
fn ingest(
    conn: &Connection,
    root: &Path,
    sources: &[Source],
    scip_langs: &BTreeSet<String>,
) -> Result<usize, EngineError> {
    let files: HashMap<String, (i64, String)> = {
        let mut stmt = conn.prepare("SELECT path, id, lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?))))?
            .collect::<Result<_, _>>()?
    };
    create_symbols(conn, root, sources, &files, scip_langs)?;

    let mut defs: BTreeMap<&str, Vec<Def>> = BTreeMap::new();
    let mut matched: Vec<Matched> = Vec::new();
    for (at, source) in sources.iter().enumerate() {
        let parsed = &source.parsed;
        let mut found = Matched {
            base: source_base(source, root),
            pending: Vec::new(),
            covered: BTreeSet::new(),
            langs: BTreeSet::new(),
        };
        for doc in &parsed.documents {
            let path = doc_path(&found.base, &doc.path);
            let Some((file, lang)) = files.get(&path) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(root.join(&path)) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let decls = load_decls(conn, *file)?;
            found.covered.insert(*file);
            found.langs.insert(lang.clone());
            for occ in doc.occurrences.iter().filter(|occ| !occ.import) {
                let Some(ident) = identifier(&lines, occ, doc.encoding) else {
                    continue;
                };
                let line = i64::from(occ.line) + 1;
                if occ.definition {
                    if let Some(decl) = declared(&decls, ident, line) {
                        defs.entry(occ.symbol.as_str()).or_default().push(Def {
                            source: at,
                            decl: decl.id,
                            file: *file,
                            kind: decl.kind.clone(),
                            name: decl.name.clone(),
                            line: decl.start,
                        });
                    }
                } else {
                    found.pending.push(Pending {
                        file: *file,
                        line,
                        ident: ident.to_string(),
                        symbol: occ.symbol.clone(),
                    });
                }
            }
        }
        matched.push(found);
    }

    // A package none of the runs defines anything of (the standard library, a
    // dependency) is outside the repo. A package named like a project of the
    // repo is inside it even when no run indexed that project.
    let own: BTreeSet<&str> = sources
        .iter()
        .flat_map(|source| source.parsed.documents.iter())
        .flat_map(|doc| doc.occurrences.iter())
        .filter(|occ| occ.definition)
        .map(|occ| package_of(&occ.symbol))
        .collect();
    let projects: BTreeSet<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT project FROM symbols WHERE project IS NOT NULL")?;
        let names: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
        names
            .into_iter()
            .map(|name| name.to_lowercase().replace('-', "_"))
            .collect()
    };

    let mut sites: HashMap<i64, FileSites> = HashMap::new();
    let mut documents = 0;
    for (at, (source, found)) in sources.iter().zip(&matched).enumerate() {
        if found.covered.is_empty() {
            continue;
        }
        let mut rows: BTreeMap<i64, BTreeSet<Row>> = BTreeMap::new();
        for site in &found.pending {
            // Two projects can define a symbol of the same name: a reference means the one its own
            // index defines, and any of the others only when its own defines none.
            let targets = defs.get(site.symbol.as_str()).map(|all| {
                let own: Vec<&Def> = all.iter().filter(|def| def.source == at).collect();
                if own.is_empty() { all.iter().collect() } else { own }
            });
            let outside = targets.is_none()
                && !own.contains(package_of(&site.symbol))
                && !projects.contains(&package_name(&site.symbol));
            if targets.is_none() && !outside {
                continue;
            }
            let held = match sites.entry(site.file) {
                Entry::Occupied(known) => known.into_mut(),
                Entry::Vacant(slot) => slot.insert(load_sites(conn, site.file)?),
            };
            let Some(here) = held.by_site.get(&(site.line, site.ident.clone())) else {
                continue;
            };
            for (src, kind) in here {
                // A site outside any declaration belongs to the file symbol, as in the resolvers.
                let Some(src) = src.or(held.file_symbol) else {
                    continue;
                };
                let ends = if outside {
                    vec![None]
                } else {
                    targets.iter().flatten().map(|target| Some(target.decl)).collect()
                };
                for dst in ends {
                    rows.entry(site.file).or_default().insert(Row {
                        src,
                        dst,
                        kind: kind.clone(),
                        name: site.ident.clone(),
                        line: site.line,
                    });
                }
            }
        }
        for doc in &source.parsed.documents {
            for (implementer, base) in &doc.implementations {
                let (Some(bases), Some(impls)) = (defs.get(base.as_str()), defs.get(implementer.as_str())) else {
                    continue;
                };
                let member = |def: &&Def| matches!(def.kind.as_str(), "method" | "function" | "member");
                for from in bases.iter().filter(member) {
                    for to in impls.iter().filter(member).filter(|to| to.decl != from.decl) {
                        rows.entry(to.file).or_default().insert(Row {
                            src: from.decl,
                            dst: Some(to.decl),
                            kind: "dispatch".to_string(),
                            name: to.name.clone(),
                            line: to.line,
                        });
                    }
                }
            }
        }
        documents += found.covered.len();
        store(conn, &found.base, &source.indexer, &found.langs, &found.covered, rows)?;
    }
    Ok(documents)
}

/// Replaces the native edges of every file under `base` in `langs`, a file at
/// a time and only when its rows differ, then notes the run.
fn store(
    conn: &Connection,
    base: &str,
    indexer: &str,
    langs: &BTreeSet<String>,
    covered: &BTreeSet<i64>,
    mut rows: BTreeMap<i64, BTreeSet<Row>>,
) -> Result<(), EngineError> {
    let scope: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id, path, lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
        let all: Vec<(i64, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<_, _>>()?;
        all.into_iter()
            .filter(|(_, path, lang)| langs.contains(lang) && (base.is_empty() || contains(base, path)))
            .map(|(id, _, _)| id)
            .collect()
    };
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut changed = false;
    for file in &scope {
        // A file this index does not have keeps whatever an earlier run stored for it.
        if !covered.contains(file) {
            tx.execute(
                "INSERT OR IGNORE INTO native_files(file_id, indexer, covered) VALUES (?1, ?2, 0)",
                params![file, indexer],
            )?;
            continue;
        }
        let new = rows.remove(file).unwrap_or_default();
        let old: BTreeSet<Row> = {
            let mut stmt =
                tx.prepare_cached("SELECT src_id, dst_id, kind, name, line FROM native_edges WHERE file_id = ?1")?;
            stmt.query_map([file], |row| {
                Ok(Row {
                    src: row.get(0)?,
                    dst: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    line: row.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?
        };
        if old != new {
            tx.execute("DELETE FROM native_edges WHERE file_id = ?1", [file])?;
            let mut insert = tx.prepare_cached(
                "INSERT INTO native_edges(src_id, dst_id, kind, name, file_id, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for row in &new {
                insert.execute(params![row.src, row.dst, row.kind, row.name, file, row.line])?;
            }
            changed = true;
        }
        tx.execute(
            "INSERT OR REPLACE INTO native_files(file_id, indexer, covered) VALUES (?1, ?2, ?3)",
            params![file, indexer, covered.contains(file)],
        )?;
    }
    let lang_list = langs.iter().cloned().collect::<Vec<_>>().join(",");
    tx.execute(
        "INSERT OR REPLACE INTO native_runs(indexer, base, langs) VALUES (?1, ?2, ?3)",
        params![indexer, base, lang_list],
    )?;
    if changed {
        let version: i64 = meta_value(&tx, "graph_version")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        set_meta(&tx, "graph_version", &(version + 1).to_string())?;
    }
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- kept indexes

/// The list of kept indexes, in `.stone/native`.
const KEPT: &str = "kept.json";

/// Present in `.stone/native` while a pass runs its indexers.
const RUNNING: &str = "running";

/// Marks a pass as running; the mark goes when the pass ends, however it ends.
struct Running(PathBuf);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Whether a pass in the background would bring the native edges up to date: one ran before,
/// files of its languages changed since, and none runs now. A mark older than four timeouts is
/// from a pass that died.
pub fn native_due(repo: &Repo) -> Result<bool, EngineError> {
    let dir = repo.store.layout.root.join(".stone").join("native");
    if !dir.join(KEPT).exists() {
        return Ok(false);
    }
    let running = std::fs::metadata(dir.join(RUNNING))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .is_some_and(|age| age < DEFAULT_TIMEOUT * 4);
    if running {
        return Ok(false);
    }
    let index = open(repo)?;
    Ok(stale_note(index.conn())?.is_some())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    indexes: Vec<KeptIndex>,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeptIndex {
    /// The file name in `.stone/native`.
    file: String,
    /// A later pass of the same indexer replaces it.
    indexer: String,
    hint: String,
    /// The content hash of each file its documents matched, when it was read.
    hashes: BTreeMap<String, String>,
}

/// An index of this pass: where it is now and the source it was read as. `copy` is set for an
/// index given with `--index`, which stays where it is; an indexer's output is moved.
struct Fresh {
    at: PathBuf,
    source: usize,
    copy: bool,
}

fn read_kept(dir: &Path) -> Kept {
    std::fs::read(dir.join(KEPT))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The code files of the graph: path to id and content hash.
fn code_files(conn: &Connection) -> Result<HashMap<String, (i64, String)>, EngineError> {
    let mut stmt = conn.prepare("SELECT path, id, hash FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
    let files = stmt
        .query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?
        .collect::<Result<_, _>>()?;
    Ok(files)
}

/// The code files added or replaced since a native run covered their language and directory:
/// path and language. A file of a language the run never had (a JavaScript config file beside
/// TypeScript) is no change, as in [`stale_note`].
fn changed_files(conn: &Connection) -> Result<Vec<(String, String)>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT f.path, f.lang FROM files f WHERE f.lang IS NOT NULL AND f.lang <> 'project' \
         AND NOT EXISTS (SELECT 1 FROM native_files n WHERE n.file_id = f.id) \
         AND EXISTS (SELECT 1 FROM native_runs r \
                     WHERE instr(',' || r.langs || ',', ',' || f.lang || ',') > 0 \
                       AND (r.base = '' OR substr(f.path, 1, length(r.base) + 1) = r.base || '/'))",
    )?;
    let files = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(files)
}

/// A kept index as a source of this pass, without the documents of files changed since it was
/// made (their ids go to `dropped`). `None` when it cannot be read.
fn load_kept(
    dir: &Path,
    root: &Path,
    kept: &KeptIndex,
    files: &HashMap<String, (i64, String)>,
    dropped: &mut BTreeSet<i64>,
) -> Option<Source> {
    let bytes = std::fs::read(dir.join(&kept.file)).ok()?;
    let mut source = Source {
        parsed: parse_scip(&bytes).ok()?,
        hint: kept.hint.clone(),
        indexer: kept.indexer.clone(),
    };
    let base = source_base(&source, root);
    source.parsed.documents.retain(|doc| {
        let path = doc_path(&base, &doc.path);
        let Some((id, now)) = files.get(&path) else {
            return false;
        };
        let same = kept.hashes.get(&path) == Some(now);
        if !same {
            dropped.insert(*id);
        }
        same
    });
    Some(source)
}

/// A file left out of a kept index gets no `native_files` row, so it reads as out of date.
fn forget_covered(conn: &Connection, dropped: &BTreeSet<i64>) -> Result<(), EngineError> {
    if dropped.is_empty() {
        return Ok(());
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    for id in dropped {
        tx.execute("DELETE FROM native_files WHERE file_id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Keeps the indexes of this pass in place of the earlier ones for the same indexer and
/// directory. Keeping is best effort: an index that cannot be moved is left out and the pass
/// still counts.
fn keep(conn: &Connection, root: &Path, dir: &Path, sources: &[Source], fresh: &[Fresh]) -> Result<(), EngineError> {
    if fresh.is_empty() || std::fs::create_dir_all(dir).is_err() {
        return Ok(());
    }
    let files = code_files(conn)?;
    let renewed: BTreeSet<(&str, &str)> = fresh
        .iter()
        .map(|run| (sources[run.source].indexer.as_str(), sources[run.source].hint.as_str()))
        .collect();
    let mut kept = read_kept(dir);
    kept.indexes.retain(|old| {
        let stays = !renewed.contains(&(old.indexer.as_str(), old.hint.as_str()));
        // An index given with --index may be one of the kept files itself.
        let read_now = fresh.iter().any(|run| run.at == dir.join(&old.file));
        if !stays && !read_now {
            let _ = std::fs::remove_file(dir.join(&old.file));
        }
        stays
    });
    let mut used: BTreeSet<String> = kept.indexes.iter().map(|old| old.file.clone()).collect();
    for run in fresh {
        let source = &sources[run.source];
        let name: String = source
            .indexer
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .collect();
        let file = (0..)
            .map(|n| format!("{name}-{n}.scip"))
            .find(|file| !used.contains(file) && !fresh.iter().any(|other| other.at == dir.join(file)))
            .unwrap_or_default();
        used.insert(file.clone());
        let to = dir.join(&file);
        let moved = if run.copy {
            std::fs::copy(&run.at, &to).map(|_| ())
        } else {
            std::fs::rename(&run.at, &to)
        };
        if moved.is_err() {
            continue;
        }
        let base = source_base(source, root);
        let hashes = source
            .parsed
            .documents
            .iter()
            .filter_map(|doc| {
                let path = doc_path(&base, &doc.path);
                let (_, hash) = files.get(&path)?;
                Some((path, hash.clone()))
            })
            .collect();
        kept.indexes.push(KeptIndex {
            file,
            indexer: source.indexer.clone(),
            hint: source.hint.clone(),
            hashes,
        });
    }
    let json = serde_json::to_vec_pretty(&kept).map_err(|err| EngineError::Invalid(err.to_string()))?;
    let _ = std::fs::write(dir.join(KEPT), json);
    Ok(())
}

/// Brings the native edges back from the kept indexes once a rebuilt code index has none: a new
/// version of stone or a new index format. A file changed since its index was made stays out and
/// is reported out of date, as after any edit.
pub(crate) fn restore(repo: &Repo, conn: &Connection) -> Result<(), EngineError> {
    let root = &repo.store.layout.root;
    let dir = root.join(".stone").join("native");
    if !dir.join(KEPT).exists()
        || meta_value(conn, "code_building")?.as_deref() == Some("1")
        || count(conn, "SELECT count(*) FROM native_files")? > 0
    {
        return Ok(());
    }
    let files = code_files(conn)?;
    let mut dropped: BTreeSet<i64> = BTreeSet::new();
    let sources: Vec<Source> = read_kept(&dir)
        .indexes
        .iter()
        .filter_map(|kept| load_kept(&dir, root, kept, &files, &mut dropped))
        .collect();
    ingest(conn, root, &sources, &scip_only_langs(repo))?;
    forget_covered(conn, &dropped)?;
    // Nothing came back: stop looking on every refresh.
    if count(conn, "SELECT count(*) FROM native_files")? == 0 {
        let _ = std::fs::remove_file(dir.join(KEPT));
    }
    Ok(())
}

// ---------------------------------------------------------------- staleness

/// "native edges are out of date for 3 files (core, graph); run `stone code native`"
/// when a run covered a language and files of it were added or replaced since.
pub(crate) fn stale_note(conn: &Connection) -> Result<Option<String>, EngineError> {
    let runs: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT base, langs FROM native_runs")?;
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    let mut projects: BTreeMap<String, usize> = BTreeMap::new();
    for (base, langs) in runs {
        let mut stmt = conn.prepare_cached(
            "SELECT ifnull((SELECT s.project FROM symbols s WHERE s.file_id = f.id AND s.project IS NOT NULL LIMIT 1), f.lang) \
             FROM files f \
             WHERE f.lang IS NOT NULL AND f.lang <> 'project' \
               AND instr(',' || ?1 || ',', ',' || f.lang || ',') > 0 \
               AND (?2 = '' OR substr(f.path, 1, length(?2) + 1) = ?2 || '/') \
               AND NOT EXISTS (SELECT 1 FROM native_files n WHERE n.file_id = f.id)",
        )?;
        for name in stmt.query_map(params![langs, base], |row| row.get::<_, String>(0))? {
            *projects.entry(name?).or_default() += 1;
        }
    }
    if projects.is_empty() {
        return Ok(None);
    }
    let files: usize = projects.values().sum();
    let mut names: Vec<&str> = projects.keys().map(String::as_str).take(3).collect();
    if projects.len() > 3 {
        names.push("...");
    }
    let noun = if files == 1 { "file" } else { "files" };
    Ok(Some(format!(
        "native edges are out of date for {files} {noun} ({}); run `stone code native` to refresh them",
        names.join(", ")
    )))
}
