//! `stone code entries`: where execution of a component starts. Entries come from two places: the
//! `entry` list an arch item declares, and detection over the code graph (a Rust `fn main`, a C#
//! `Main`, top-level statements, a `package.json` `bin` or `main`, a file that starts a server, a
//! Python `__main__` guard, a Rust library's `src/lib.rs`). Detection reads the index, and the few source lines the graph does
//! not keep (the Python guard, a top-level server call).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::arch_graph::is_arch;
use crate::check::Problem;
use crate::code::{CodeQuery, effective_limit, open};
use crate::code_index::building_note;
use crate::code_rank::is_test_path;
use crate::owner::arch_elements;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::tiers;

/// A sentinel for a declared entry the code graph no longer holds.
const GONE: &str = "gone";
const DECLARED: &str = "declared";
/// The kind of a Rust library crate root.
pub(crate) const LIBRARY: &str = "library";
const SERVER_CALLS: [&str; 2] = ["listen", "createServer"];
/// Folders a published `package.json` path usually points into instead of the sources.
const BUILD_DIRS: [&str; 4] = ["dist", "build", "lib", "out"];
/// A site sits at file level when it has no enclosing declaration besides the file or a namespace.
const FILE_LEVEL: &str = "(s.id IS NULL OR s.kind IN ('file', 'module'))";

/// One entry as `code entries` and `code explain` list it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntryRow {
    /// `path#Symbol` or `path`: the text an `entry` list holds.
    pub target: String,
    /// `main`, `program`, `package`, `server` or `library` for a detected entry;
    /// `declared` for a declared one the detectors do not see; `gone` for a declared one the code
    /// graph no longer holds.
    pub kind: String,
    /// What was seen, such as `fn main`; empty when the kind says it all.
    pub detail: String,
    /// The component's `entry` list names it.
    pub declared: bool,
}

impl EntryRow {
    /// The printed row: `target  kind  detail`, with `declared` when the list names it too.
    pub fn line(&self) -> String {
        let mut out = format!("{}  {}", self.target, self.kind);
        if !self.detail.is_empty() {
            out.push_str(&format!("  {}", self.detail));
        }
        if self.declared && self.kind != DECLARED {
            out.push_str("  declared");
        }
        out
    }
}

/// The entries of the repository grouped by the component that owns their file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Entries {
    pub groups: Vec<EntryGroup>,
    pub truncated: bool,
    /// Entries the limit left out.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of what the limit left out, as one line per dropped row.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntryGroup {
    /// The component's label, or `(unowned)` for entries no component claims.
    pub component: String,
    pub entries: Vec<EntryRow>,
}

/// An entry the detectors saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub path: String,
    /// The qualified name of the symbol, when the entry is one.
    pub symbol: Option<String>,
    pub line: u32,
    pub kind: &'static str,
    pub detail: String,
}

impl Found {
    pub(crate) fn target(&self) -> String {
        match &self.symbol {
            Some(symbol) => format!("{}#{symbol}", self.path),
            None => self.path.clone(),
        }
    }
}

// ---------------------------------------------------------------- sources

/// Source files read on demand, once each.
struct Sources<'a> {
    root: &'a Path,
    files: HashMap<String, Vec<String>>,
}

impl<'a> Sources<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: HashMap::new(),
        }
    }

    fn lines(&mut self, path: &str) -> &[String] {
        self.files.entry(path.to_string()).or_insert_with(|| {
            std::fs::read_to_string(self.root.join(path))
                .map(|text| text.lines().map(str::to_string).collect())
                .unwrap_or_default()
        })
    }
}

/// `'a', 'b'` for an SQL `IN` list of fixed names.
fn quoted<'a>(names: impl Iterator<Item = &'a str>) -> String {
    names.map(|name| format!("'{name}'")).collect::<Vec<_>>().join(", ")
}

fn rows<T>(
    conn: &Connection,
    sql: &str,
    map: impl FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>, EngineError> {
    let mut stmt = conn.prepare(sql)?;
    let found = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(found)
}

// ---------------------------------------------------------------- detection

/// Every entry in the files `keep` accepts, sorted by path, line and kind. Test code is never an
/// entry: a test file with a `__main__` guard or a `Main` only runs its tests.
pub(crate) fn detect(conn: &Connection, root: &Path, keep: &dyn Fn(&str) -> bool) -> Result<Vec<Found>, EngineError> {
    let keep = &|path: &str| keep(path) && !is_test_path(path);
    let mut sources = Sources::new(root);
    let mut out = Vec::new();
    rust_mains(conn, keep, &mut out)?;
    rust_libs(conn, root, keep, &mut out)?;
    csharp(conn, keep, &mut out)?;
    javascript(conn, keep, &mut sources, &mut out)?;
    package_json(conn, root, keep, &mut out)?;
    python(conn, keep, &mut sources, &mut out)?;
    out.sort_by(|a, b| (&a.path, a.line, a.kind, &a.symbol).cmp(&(&b.path, b.line, b.kind, &b.symbol)));
    out.dedup();
    Ok(out)
}

fn rust_mains(conn: &Connection, keep: &dyn Fn(&str) -> bool, out: &mut Vec<Found>) -> Result<(), EngineError> {
    let mains = rows(
        conn,
        "SELECT f.path, s.name, s.line_start FROM symbols s JOIN files f ON f.id = s.file_id \
         LEFT JOIN symbols p ON p.id = s.parent_id \
         WHERE s.lang = 'rust' AND s.kind = 'function' AND s.name = 'main' \
         AND (p.id IS NULL OR p.kind IN ('file', 'module'))",
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
            ))
        },
    )?;
    for (path, name, line) in mains {
        // A build script runs at compile time; it starts nothing a user runs. The module path in a
        // qualified name only repeats the file path, so the entry is named `main`.
        if keep(&path) && path.rsplit('/').next() != Some("build.rs") {
            out.push(Found {
                path,
                symbol: Some(name),
                line,
                kind: "main",
                detail: "fn main".into(),
            });
        }
    }
    Ok(())
}

/// The root file of a library crate: a `src/lib.rs` with a `Cargo.toml` in the folder above `src/`.
fn rust_libs(
    conn: &Connection,
    root: &Path,
    keep: &dyn Fn(&str) -> bool,
    out: &mut Vec<Found>,
) -> Result<(), EngineError> {
    let libs = rows(
        conn,
        "SELECT path FROM files WHERE lang = 'rust' AND (path = 'src/lib.rs' OR path LIKE '%/src/lib.rs')",
        |row| row.get::<_, String>(0),
    )?;
    for path in libs {
        let folder = path.strip_suffix("src/lib.rs").unwrap_or("").trim_end_matches('/');
        if keep(&path) && root.join(folder).join("Cargo.toml").is_file() {
            out.push(Found {
                path,
                symbol: None,
                line: 1,
                kind: LIBRARY,
                detail: String::new(),
            });
        }
    }
    Ok(())
}

fn csharp(conn: &Connection, keep: &dyn Fn(&str) -> bool, out: &mut Vec<Found>) -> Result<(), EngineError> {
    let mains = rows(
        conn,
        "SELECT f.path, s.qname, s.line_start, s.signature FROM symbols s JOIN files f ON f.id = s.file_id \
         WHERE s.lang = 'csharp' AND s.kind = 'method' AND s.name = 'Main'",
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    for (path, qname, line, signature) in mains {
        if keep(&path) && signature.split_whitespace().any(|word| word == "static") {
            out.push(Found {
                path,
                symbol: Some(qname),
                line,
                kind: "main",
                detail: "static Main".into(),
            });
        }
    }

    // Statements outside any type run when the program starts.
    let top_level = rows(
        conn,
        &format!(
            "SELECT f.path, min(r.line) FROM refs r JOIN files f ON f.id = r.file_id \
             LEFT JOIN symbols s ON s.id = r.src_symbol_id \
             WHERE f.lang = 'csharp' AND r.kind = 'calls' AND {FILE_LEVEL} GROUP BY f.path"
        ),
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
    )?;
    for (path, line) in top_level {
        if keep(&path) {
            out.push(Found {
                path,
                symbol: None,
                line,
                kind: "program",
                detail: "top-level statements".into(),
            });
        }
    }
    Ok(())
}

fn javascript(
    conn: &Connection,
    keep: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
    out: &mut Vec<Found>,
) -> Result<(), EngineError> {
    // A file that starts a server in a top-level statement is the way that server is run. A call
    // inside an anonymous callback (a test hook, a handler) has no declaration of its own, so the
    // graph reads it as file level; the source tells the two apart.
    let calls = rows(
        conn,
        &format!(
            "SELECT f.path, r.line, r.name FROM refs r JOIN files f ON f.id = r.file_id \
             LEFT JOIN symbols s ON s.id = r.src_symbol_id \
             WHERE f.lang IN ('typescript', 'javascript') AND r.kind = 'calls' AND r.name IN ({}) \
             AND {FILE_LEVEL} ORDER BY f.path, r.line",
            quoted(SERVER_CALLS.into_iter())
        ),
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let mut starts: Vec<(String, u32, Vec<String>)> = Vec::new();
    for (path, line, name) in calls {
        if !keep(&path) || !top_level_statement(sources, &path, line) {
            continue;
        }
        match starts.last_mut() {
            Some((last, _, names)) if *last == path => {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            _ => starts.push((path, line, vec![name])),
        }
    }
    for (path, line, names) in starts {
        out.push(Found {
            path,
            symbol: None,
            line,
            kind: "server",
            detail: names.join(", "),
        });
    }
    Ok(())
}

/// True when the statement on `line` starts at the left edge of the file, or sits directly in a
/// `require.main` / `import.meta` guard that does. An indented line elsewhere is inside a block.
fn top_level_statement(sources: &mut Sources, path: &str, line: u32) -> bool {
    let lines = sources.lines(path);
    let at = (line as usize).saturating_sub(1);
    let indented = |text: &str| text.starts_with([' ', '\t']);
    let Some(text) = lines.get(at) else {
        return false;
    };
    if !indented(text) {
        return true;
    }
    let opener = lines[..at]
        .iter()
        .rev()
        .find(|text| !text.trim().is_empty() && !indented(text));
    opener.is_some_and(|text| text.contains("require.main") || text.contains("import.meta"))
}

/// A `package.json` `bin` or `main`, mapped to the code file it names in the graph.
fn package_json(
    conn: &Connection,
    root: &Path,
    keep: &dyn Fn(&str) -> bool,
    out: &mut Vec<Found>,
) -> Result<(), EngineError> {
    let manifests = rows(
        conn,
        "SELECT path FROM files WHERE lang = 'project' AND (path = 'package.json' OR path LIKE '%/package.json') ORDER BY path",
        |row| row.get::<_, String>(0),
    )?;
    for manifest in manifests {
        let Ok(text) = std::fs::read_to_string(root.join(&manifest)) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let dir = manifest.rsplit_once('/').map_or("", |(dir, _)| dir);
        let mut named: Vec<(String, String)> = Vec::new();
        if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
            named.push(("main".into(), main.into()));
        }
        match json.get("bin") {
            Some(serde_json::Value::String(bin)) => named.push(("bin".into(), bin.clone())),
            Some(serde_json::Value::Object(map)) => {
                for (name, value) in map {
                    if let Some(bin) = value.as_str() {
                        named.push((format!("bin {name}"), bin.to_string()));
                    }
                }
            }
            _ => {}
        }
        for (label, value) in named {
            let relative = value.trim_start_matches("./");
            let wanted = if dir.is_empty() {
                relative.to_string()
            } else {
                format!("{dir}/{relative}")
            };
            let Some(path) = code_file_for(conn, &wanted)? else {
                continue;
            };
            if keep(&path) {
                out.push(Found {
                    path,
                    symbol: None,
                    line: 1,
                    kind: "package",
                    detail: format!("{manifest} {label}"),
                });
            }
        }
    }
    Ok(())
}

/// The indexed file a `package.json` path names: the path itself, else its source twin when it points
/// into a build folder (`dist/cli.js` for `src/cli.ts`).
fn code_file_for(conn: &Connection, wanted: &str) -> Result<Option<String>, EngineError> {
    let exists = |path: &str| -> Result<bool, EngineError> {
        let found: Option<i64> = conn
            .query_row(
                "SELECT id FROM files WHERE path = ?1 AND lang IN ('typescript', 'javascript')",
                [path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    };
    if exists(wanted)? {
        return Ok(Some(wanted.to_string()));
    }
    let (dir, file) = wanted.rsplit_once('/').map_or(("", wanted), |(d, f)| (d, f));
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let Some(at) = parts.iter().position(|p| BUILD_DIRS.contains(p)) else {
        return Ok(None);
    };
    parts[at] = "src";
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let folder = parts.join("/");
    for ext in ["ts", "tsx", "js", "mjs", "cjs"] {
        let candidate = if folder.is_empty() {
            format!("{stem}.{ext}")
        } else {
            format!("{folder}/{stem}.{ext}")
        };
        if exists(&candidate)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn python(
    conn: &Connection,
    keep: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
    out: &mut Vec<Found>,
) -> Result<(), EngineError> {
    // The guard's body is a call at file level, so only files with one are worth reading.
    let candidates = rows(
        conn,
        &format!(
            "SELECT DISTINCT f.path FROM files f JOIN refs r ON r.file_id = f.id \
             LEFT JOIN symbols s ON s.id = r.src_symbol_id WHERE f.lang = 'python' AND {FILE_LEVEL} ORDER BY f.path"
        ),
        |row| row.get::<_, String>(0),
    )?;
    for path in candidates {
        if !keep(&path) {
            continue;
        }
        let guard = sources
            .lines(&path)
            .iter()
            .position(|line| line.starts_with("if __name__") && line.contains("__main__"));
        if let Some(index) = guard {
            out.push(Found {
                path,
                symbol: None,
                line: index as u32 + 1,
                kind: "main",
                detail: "__main__ guard".into(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- declared entries

/// Whether a declared entry still names something in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryState {
    Found,
    NoFile,
    NoSymbol,
}

/// Splits `path#Symbol`; the symbol is `None` for a bare path.
fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.trim().split_once('#') {
        Some((path, symbol)) => (path.trim(), Some(symbol.trim()).filter(|s| !s.is_empty())),
        None => (target.trim(), None),
    }
}

/// A declared `Symbol` names a symbol by its qualified name, its name, or the tail of its qualified name.
fn symbol_matches(qname: &str, name: &str, wanted: &str) -> bool {
    qname == wanted
        || name == wanted
        || qname.ends_with(&format!(".{wanted}"))
        || qname.ends_with(&format!("::{wanted}"))
}

pub(crate) fn entry_state(conn: &Connection, target: &str) -> Result<EntryState, EngineError> {
    let (path, symbol) = split_target(target);
    let file: Option<i64> = conn
        .query_row(
            "SELECT id FROM files WHERE path = ?1 AND lang IS NOT NULL AND lang <> 'project'",
            [path],
            |row| row.get(0),
        )
        .optional()?;
    let Some(file) = file else {
        return Ok(EntryState::NoFile);
    };
    let Some(wanted) = symbol else {
        return Ok(EntryState::Found);
    };
    let mut stmt = conn.prepare("SELECT qname, name FROM symbols WHERE file_id = ?1 AND kind <> 'file'")?;
    let names = stmt
        .query_map([file], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        if names.iter().any(|(qname, name)| symbol_matches(qname, name, wanted)) {
            EntryState::Found
        } else {
            EntryState::NoSymbol
        },
    )
}

/// True when the declared entry names this detection: the same file, and the same symbol when it names one.
fn covers(declared: &str, found: &Found) -> bool {
    let (path, symbol) = split_target(declared);
    path == found.path
        && match (symbol, &found.symbol) {
            (None, _) => true,
            (Some(wanted), Some(qname)) => {
                let name = qname.rsplit(['.', ':']).next().unwrap_or(qname);
                symbol_matches(qname, name, wanted)
            }
            (Some(_), None) => false,
        }
}

/// The rows for one component: its declared entries first, then the detected ones in files `owns`
/// accepts that no declared entry covers.
pub(crate) fn component_rows(
    conn: &Connection,
    found: &[Found],
    declared: &[String],
    owns: &dyn Fn(&str) -> bool,
) -> Result<Vec<EntryRow>, EngineError> {
    let mine: Vec<&Found> = found.iter().filter(|f| owns(&f.path)).collect();
    let mut covered = vec![false; mine.len()];
    let mut out = Vec::new();
    for target in declared {
        let hits: Vec<usize> = (0..mine.len()).filter(|&i| covers(target, mine[i])).collect();
        for &i in &hits {
            covered[i] = true;
        }
        let (kind, detail) = match hits.first() {
            Some(&i) => (mine[i].kind.to_string(), mine[i].detail.clone()),
            None => match entry_state(conn, target)? {
                EntryState::Found => (DECLARED.to_string(), String::new()),
                _ => (GONE.to_string(), "not in the code graph".to_string()),
            },
        };
        out.push(EntryRow {
            target: target.trim().to_string(),
            kind,
            detail,
            declared: true,
        });
    }
    for (found, _) in mine.iter().zip(&covered).filter(|(_, covered)| !**covered) {
        out.push(EntryRow {
            target: found.target(),
            kind: found.kind.to_string(),
            detail: found.detail.clone(),
            declared: false,
        });
    }
    Ok(out)
}

/// The detected entries of the files in `files` as a `target` list for a draft, without repeats.
pub(crate) fn targets_in(found: &[Found], files: &BTreeSet<&str>) -> Vec<String> {
    let targets: BTreeSet<String> = found
        .iter()
        .filter(|f| files.contains(f.path.as_str()))
        .map(Found::target)
        .collect();
    targets.into_iter().collect()
}

// ---------------------------------------------------------------- check

/// True when the code index holds at least one source file; checks against it say nothing otherwise.
pub(crate) fn has_code(conn: &Connection) -> Result<bool, EngineError> {
    let files: i64 = conn.query_row(
        "SELECT count(*) FROM files WHERE lang IS NOT NULL AND lang <> 'project'",
        [],
        |row| row.get(0),
    )?;
    Ok(files > 0)
}

/// `entry` warnings: a declared file or symbol the code graph no longer holds. Skipped when the code
/// index holds no files.
pub(crate) fn entry_problems(
    snapshot: &Snapshot,
    conn: &Connection,
    out: &mut Vec<Problem>,
) -> Result<(), EngineError> {
    if !has_code(conn)? {
        return Ok(());
    }
    for entry in &snapshot.entries {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        if !is_arch(item.item_type()) {
            continue;
        }
        let line = doc.key_line("entry").unwrap_or(1);
        for target in &item.fields.entry {
            let message = match entry_state(conn, target)? {
                EntryState::Found => continue,
                EntryState::NoFile => format!(
                    "entry `{}` names a file the code graph does not hold; fix the path or remove it from `entry`",
                    target.trim()
                ),
                EntryState::NoSymbol => format!(
                    "entry `{}` names a symbol that is not in that file; run `stone code entries {}` and update `entry`",
                    target.trim(),
                    snapshot.label(entry)
                ),
            };
            out.push(Problem::warning(&entry.path, line, "entry", message));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- query

/// Every component's entries, or one component's when `q.target` names it. Entries in files no
/// component owns come last under `(unowned)`.
pub fn entries(repo: &Repo, q: &CodeQuery) -> Result<Entries, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let snapshot = repo.snapshot()?;
    let elements = arch_elements(&snapshot, &mut Vec::new());
    let found = detect(conn, &repo.store.layout.root, &|_| true)?;

    let wanted = if q.target.trim().is_empty() {
        None
    } else {
        let (entry, _) = snapshot.resolve(&q.target)?;
        if !entry.item.as_ref().is_some_and(|item| is_arch(item.item_type())) {
            return Err(EngineError::Invalid(format!(
                "`{}` is not an architecture element; give a component id such as arch/core",
                q.target
            )));
        }
        Some(entry.id.as_str().to_string())
    };

    let mut groups = Vec::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        if !is_arch(item.item_type()) || wanted.as_deref().is_some_and(|id| id != entry.id.as_str()) {
            continue;
        }
        let element = elements.iter().find(|e| e.id == entry.id.as_str());
        let owns = |path: &str| element.is_some_and(|e| e.owns(path));
        let rows = component_rows(conn, &found, &item.fields.entry, &owns)?;
        if !rows.is_empty() {
            groups.push(EntryGroup {
                component: snapshot.label(entry),
                entries: rows,
            });
        }
    }
    if wanted.is_none() {
        let unowned = |path: &str| !elements.iter().any(|e| e.globs.is_match(path));
        let rows = component_rows(conn, &found, &[], &unowned)?;
        if !rows.is_empty() {
            groups.push(EntryGroup {
                component: "(unowned)".into(),
                entries: rows,
            });
        }
    }

    let mut room = effective_limit(q.limit);
    let mut omitted = 0;
    let mut dropped = Vec::new();
    for group in &mut groups {
        if group.entries.len() > room {
            omitted += group.entries.len() - room;
            dropped.extend(group.entries[room..].iter().map(EntryRow::line));
            group.entries.truncate(room);
        }
        room -= group.entries.len();
    }
    groups.retain(|group| !group.entries.is_empty());
    Ok(Entries {
        groups,
        truncated: omitted > 0,
        omitted,
        left_out: tiers::left_out("entries", &dropped),
        note: building_note(conn)?,
    })
}
