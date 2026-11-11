//! The code graph inside refresh.
//!
//! Code files (git tracked plus untracked and not ignored, under `code_roots`,
//! a supported extension) and project files (`*.csproj`, `Cargo.toml`, ...)
//! are `files` rows with `lang` set. The pass has two halves:
//!
//! 1. Facts. Changed files are read, hashed and parsed in parallel, then
//!    written serially, at most `batch_files` per transaction. `meta.code_building`
//!    is 1 from the first write until the pass ends, so a pass that dies after
//!    any batch is noticed by the next one.
//! 2. Edges. The resolver world is rebuilt from rows and the files whose edges
//!    can have changed are re-resolved: the changed files, every file that names
//!    a symbol the changed files declared or declare now, and (for a project file
//!    edit) every file of the languages that project file governs. A pass that
//!    resumes after an interruption re-resolves everything, because what the
//!    interrupted pass knew is gone. A file's edges are rewritten only when they
//!    differ, so the result equals a full rebuild and a second pass writes nothing.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use carvestone_graph::{
    Bucket, CODE_GRAPH_VERSION, Decl, DeclKind, Edge, FileFacts, ImportDecl, Lang, Node, PathAlias, ProjectFacts,
    ProjectKind, ProjectReference, RefKind, RefSite, World, dispatch, extract, lang_of, project_facts, resolver_for,
};
use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::check::Problem;
use crate::refresh::in_batches;
use crate::repo::{EngineError, Repo};

/// The `files.lang` of a project file.
const PROJECT_LANG: &str = "project";

/// Directories the fallback walk (no git) never enters.
const SKIPPED_DIRS: [&str; 4] = [".git", ".stone", "node_modules", "target"];

// ---------------------------------------------------------------- listing

enum Kind {
    Code(Lang),
    /// A file of a language only a `scip_indexes` entry lists; it holds the language name.
    Scip(String),
    Project,
}

struct Candidate {
    path: String,
    kind: Kind,
    abs: PathBuf,
    mtime_ns: i64,
    size: i64,
}

struct Known {
    id: i64,
    hash: String,
    mtime_ns: i64,
    size: i64,
}

/// The files of the code roots as git lists them, root-relative.
pub(crate) struct Listing {
    /// Files each repo around a code root lists, kept to the ones that repo itself holds (a
    /// deeper repo lists its own).
    pub paths: Vec<String>,
    /// What git could not list: `Some(dir)` for a repo whose `ls-files` failed, `None` for
    /// paths held by no repo (a code root outside any repo).
    pub unlisted: Vec<Option<String>>,
}

/// Asks each repo around the code roots for its own files: tracked ones, and with `untracked`
/// the untracked ones that are not ignored too. Every layout takes this path; a one-repo root
/// asks one repo.
pub(crate) fn list_files(repo: &Repo, untracked: bool) -> Listing {
    let repos = repo.repos();
    let mut listing = Listing {
        paths: Vec::new(),
        unlisted: Vec::new(),
    };
    let mut asked: BTreeSet<&str> = BTreeSet::new();
    let mut outside = false;
    for prefix in root_prefixes(&repo.store.config.code_roots) {
        let dir = prefix.trim_end_matches('/');
        outside |= repos.repo_of(dir).is_none();
        for one in repos.around(dir) {
            if !asked.insert(one.dir()) {
                continue;
            }
            match one.git().ls_files(untracked) {
                Ok(found) => listing.paths.extend(found.into_iter().filter_map(|path| {
                    // A nested repo that is not a submodule shows as `dir/`.
                    if path.ends_with('/') {
                        return None;
                    }
                    let path = repos.to_root(one, &path);
                    repos
                        .repo_of(&path)
                        .is_some_and(|(owner, _)| owner.dir() == one.dir())
                        .then_some(path)
                })),
                Err(_) => listing.unlisted.push(Some(one.dir().to_string())),
            }
        }
    }
    if outside {
        listing.unlisted.push(None);
    }
    listing
}

/// Files under the repo `owner` (the whole root for `None`: paths in no repo) found by walking
/// the folders, for repos git cannot list.
fn walk_unlisted(repo: &Repo, owner: Option<&str>, out: &mut Vec<String>) {
    let repos = repo.repos();
    let start = owner.unwrap_or("");
    let mut found = Vec::new();
    walk(
        &repo.store.layout.root.join(start),
        start,
        // Another repo is listed on its own.
        &|path| repos.all().iter().any(|r| r.dir() == path && r.dir() != start),
        &mut found,
    );
    out.extend(
        found
            .into_iter()
            .filter(|path| repos.repo_of(path).map(|(one, _)| one.dir()) == owner),
    );
}

fn walk(dir: &Path, rel: &str, skip: &dyn Fn(&str) -> bool, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) && !skip(&path) {
                walk(&entry.path(), &path, skip, out);
            }
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// `code_roots` entries as path prefixes ending in `/`; an empty prefix is the whole repo.
pub(crate) fn root_prefixes(roots: &[String]) -> Vec<String> {
    roots
        .iter()
        .map(|root| {
            let root = root.trim().trim_start_matches("./").trim_end_matches('/');
            if root.is_empty() || root == "." {
                String::new()
            } else {
                format!("{root}/")
            }
        })
        .collect()
}

fn candidates(repo: &Repo) -> Vec<Candidate> {
    let prefixes = root_prefixes(&repo.store.config.code_roots);
    if prefixes.is_empty() {
        return Vec::new();
    }
    let root = &repo.store.layout.root;
    let listing = list_files(repo, true);
    let mut paths = listing.paths;
    for owner in &listing.unlisted {
        walk_unlisted(repo, owner.as_deref(), &mut paths);
    }
    paths.sort();
    paths.dedup();

    let mut out = Vec::new();
    for path in paths {
        if path.starts_with("stone/") || path.starts_with(".stone/") {
            continue;
        }
        let kind = if ProjectKind::of(&path).is_some() {
            Kind::Project
        } else if let Some(lang) = lang_of(&path)
            && prefixes.iter().any(|prefix| path.starts_with(prefix.as_str()))
        {
            Kind::Code(lang)
        } else if let Some(language) = scip_language(repo, &path)
            && prefixes.iter().any(|prefix| path.starts_with(prefix.as_str()))
        {
            // After the built-in branch, so a built-in parser wins for an extension it reads.
            Kind::Scip(language)
        } else {
            continue;
        };
        let abs = root.join(&path);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as i64);
        out.push(Candidate {
            path,
            kind,
            abs,
            mtime_ns,
            size: meta.len() as i64,
        });
    }
    out
}

/// The language a `scip_indexes` entry gives to the extension of `path`.
fn scip_language(repo: &Repo, path: &str) -> Option<String> {
    let ext = Path::new(path).extension()?.to_str()?;
    repo.store
        .config
        .scip_indexes
        .iter()
        .find(|entry| entry.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        .map(|entry| entry.language.clone())
}

/// The language of each code file under the code roots, from the file list alone: nothing is parsed.
/// Files of a `scip_indexes` language are not listed: no built-in indexer runs for them.
pub(crate) fn code_langs(repo: &Repo) -> Vec<Lang> {
    candidates(repo)
        .into_iter()
        .filter_map(|cand| match cand.kind {
            Kind::Code(lang) => Some(lang),
            Kind::Scip(_) | Kind::Project => None,
        })
        .collect()
}

/// The project folders of the code: the repos below the root that hold indexed files, one per
/// line. A one-repo root, or code in no repo, has none; modules treat the whole root as one.
fn project_dirs(repo: &Repo, cands: &[Candidate]) -> String {
    let repos = repo.repos();
    let dirs: BTreeSet<&str> = cands
        .iter()
        .filter_map(|c| repos.repo_of(&c.path))
        .map(|(one, _)| one.dir())
        .filter(|dir| !dir.is_empty())
        .collect();
    dirs.into_iter().collect::<Vec<_>>().join("\n")
}

/// Records the project folders in `meta.code_projects`. A change moves `graph_version`, so the
/// modules are clustered again.
fn sync_project_dirs(conn: &Connection, dirs: &str) -> Result<(), EngineError> {
    let stored = meta_value(conn, "code_projects")?.unwrap_or_default();
    if stored == dirs {
        return Ok(());
    }
    set_meta(conn, "code_projects", dirs)?;
    let version: i64 = meta_value(conn, "graph_version")?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    set_meta(conn, "graph_version", &(version + 1).to_string())
}

/// The project folders `sync_project_dirs` recorded.
pub(crate) fn stored_project_dirs(conn: &Connection) -> Result<Vec<String>, EngineError> {
    Ok(meta_value(conn, "code_projects")?
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect())
}

fn known_code_files(conn: &Connection) -> Result<HashMap<String, Known>, EngineError> {
    let mut stmt = conn.prepare("SELECT path, id, hash, mtime_ns, size FROM files WHERE lang IS NOT NULL")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Known {
                id: row.get(1)?,
                hash: row.get(2)?,
                mtime_ns: row.get(3)?,
                size: row.get(4)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- names

fn kind_lang(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Csproj => "csharp",
        ProjectKind::Cargo => "rust",
        ProjectKind::Npm | ProjectKind::TsConfig => "typescript",
        ProjectKind::PyProject => "python",
        ProjectKind::Pubspec => "dart",
    }
}

/// The languages a project file governs.
fn governed_langs(kind: ProjectKind) -> &'static [Lang] {
    match kind {
        ProjectKind::Csproj => &[Lang::CSharp],
        ProjectKind::Cargo => &[Lang::Rust],
        ProjectKind::Npm | ProjectKind::TsConfig => &[Lang::TypeScript, Lang::JavaScript],
        ProjectKind::PyProject => &[Lang::Python],
        ProjectKind::Pubspec => &[Lang::Dart],
    }
}

/// The project kinds whose root a file of `lang` sits under.
fn governing_kinds(lang: Lang) -> &'static [ProjectKind] {
    match lang {
        Lang::CSharp => &[ProjectKind::Csproj],
        Lang::Rust => &[ProjectKind::Cargo],
        Lang::TypeScript | Lang::JavaScript => &[ProjectKind::Npm, ProjectKind::TsConfig],
        Lang::Python => &[ProjectKind::PyProject],
        Lang::Dart => &[ProjectKind::Pubspec],
    }
}

pub(crate) fn lang_from(name: &str) -> Option<Lang> {
    [
        Lang::CSharp,
        Lang::Rust,
        Lang::TypeScript,
        Lang::JavaScript,
        Lang::Python,
        Lang::Dart,
    ]
    .into_iter()
    .find(|lang| lang.as_str() == name)
}

fn decl_kind(name: &str) -> DeclKind {
    match name {
        "project" => DeclKind::Project,
        "module" => DeclKind::Module,
        "file" => DeclKind::File,
        "type" => DeclKind::Type,
        "function" => DeclKind::Function,
        "method" => DeclKind::Method,
        _ => DeclKind::Member,
    }
}

fn ref_kind(name: &str) -> RefKind {
    match name {
        "imports" => RefKind::Imports,
        "calls" => RefKind::Calls,
        "inherits" => RefKind::Inherits,
        "implements" => RefKind::Implements,
        _ => RefKind::References,
    }
}

pub(crate) fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || (path.starts_with(root) && path.as_bytes().get(root.len()) == Some(&b'/'))
}

/// The name of the project a file belongs to: the nearest project file of a
/// kind that governs its language (an npm package beats a tsconfig next to it).
fn governing_project(world: &World, lang: Lang, path: &str) -> Option<String> {
    let kinds = governing_kinds(lang);
    world
        .projects()
        .iter()
        .filter(|project| kinds.contains(&project.kind) && contains(&project.root, path))
        .max_by_key(|&project| {
            (
                project.root.len(),
                project.kind == ProjectKind::Npm,
                Reverse(project.path.as_str()),
            )
        })
        .map(|project| project.name.clone())
}

// ---------------------------------------------------------------- meta

pub(crate) fn meta_value(conn: &Connection, key: &str) -> Result<Option<String>, EngineError> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| row.get(0))
        .optional()?)
}

pub(crate) fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), EngineError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// "code index is still building (n of m files); results may be incomplete"
/// while a pass is unfinished; else the native pass's "out of date" line when
/// files changed since it ran; else `None`. Code queries print it.
pub fn building_note(conn: &Connection) -> Result<Option<String>, EngineError> {
    if meta_value(conn, "code_building")?.as_deref() != Some("1") {
        return crate::code_native::stale_note(conn);
    }
    let total: i64 = meta_value(conn, "code_total")?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let done: i64 = conn.query_row("SELECT count(*) FROM files WHERE lang IS NOT NULL", [], |row| {
        row.get(0)
    })?;
    Ok(Some(format!(
        "code index is still building ({} of {total} files); results may be incomplete",
        done.min(total)
    )))
}

// ---------------------------------------------------------------- pass

enum Parsed {
    Same,
    Code(FileFacts),
    /// A file of a `scip_indexes` language: its language and its one file symbol.
    Scip(String, Decl),
    Project(ProjectFacts),
}

/// The parts of one file's facts that other files' resolution can depend on.
#[derive(Default)]
struct Shape {
    /// (qname, name, kind, parent qname) of each symbol, counted: another
    /// declaration under the same qname is an overload, which changes how
    /// exact its callers' edges are. The signature itself is not compared:
    /// resolution is by name, so a signature-only edit moves no edge.
    symbols: BTreeMap<(String, String, String, String), usize>,
    /// (kind, name, qualifier, scope name) of each base-list entry and typed
    /// binding; a change moves what a receiver of the scope's type resolves to.
    ties: BTreeSet<(String, String, String, String)>,
}

impl Shape {
    fn of(facts: &FileFacts) -> Shape {
        let mut shape = Shape::default();
        for decl in facts
            .decls
            .iter()
            .filter(|d| !matches!(d.kind, DeclKind::File | DeclKind::Project))
        {
            let parent = decl
                .parent
                .and_then(|p| facts.decls.get(p))
                .map_or("", |p| p.qname.as_str());
            *shape
                .symbols
                .entry((
                    decl.qname.clone(),
                    decl.name.clone(),
                    decl.kind.as_str().to_string(),
                    parent.to_string(),
                ))
                .or_default() += 1;
        }
        for site in &facts.refs {
            if let Some(scope) = site.scope.and_then(|s| facts.decls.get(s))
                && site.kind != RefKind::Calls
                && site.kind != RefKind::Imports
            {
                shape.ties.insert((
                    site.kind.as_str().to_string(),
                    site.name.clone(),
                    site.qualifier.clone().unwrap_or_default(),
                    scope.name.clone(),
                ));
            }
        }
        shape
    }

    /// The names whose users must be looked at again when `self` becomes `other`.
    fn names_changed<'a>(&'a self, other: &'a Shape) -> impl Iterator<Item = &'a String> {
        self.symbols
            .iter()
            .filter(|(key, count)| other.symbols.get(*key) != Some(*count))
            .chain(
                other
                    .symbols
                    .iter()
                    .filter(|(key, count)| self.symbols.get(*key) != Some(*count)),
            )
            .map(|(key, _)| &key.1)
            .chain(self.ties.symmetric_difference(&other.ties).map(|tie| &tie.3))
    }
}

/// What the facts half learned, for the edges half.
#[derive(Default)]
struct Pass {
    /// Names of symbols added, removed or changed, and of types whose bases or
    /// typed bindings changed: files naming them are re-resolved.
    names: BTreeSet<String>,
    /// Files with an edge into a file that was replaced; the replace dropped it.
    inbound: BTreeSet<i64>,
    /// Rows of files whose facts were replaced.
    changed: BTreeSet<i64>,
    /// Languages whose files all need re-resolving (project file edit, `global using`).
    dirty_langs: BTreeSet<Lang>,
    projects_dirty: bool,
    replaced: usize,
}

fn parse(cand: &Candidate, known: Option<&Known>) -> Option<(String, Parsed)> {
    let bytes = std::fs::read(&cand.abs).ok()?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if known.is_some_and(|k| k.hash == hash) {
        return Some((hash, Parsed::Same));
    }
    let text = String::from_utf8_lossy(&bytes);
    let parsed = match &cand.kind {
        Kind::Code(lang) => Parsed::Code(extract(*lang, &cand.path, &text)),
        Kind::Scip(language) => {
            let name = cand.path.rsplit('/').next().unwrap_or(&cand.path);
            let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
            let file = Decl {
                name: stem.to_string(),
                qname: cand.path.clone(),
                kind: DeclKind::File,
                parent: None,
                signature: String::new(),
                doc: None,
                line_start: 1,
                line_end: u32::try_from(text.lines().count().max(1)).unwrap_or(u32::MAX),
            };
            Parsed::Scip(language.clone(), file)
        }
        Kind::Project => Parsed::Project(project_facts(&cand.path, &text)?),
    };
    Some((hash, parsed))
}

/// Rows written by another extractor or resolver version are dropped (the
/// cascade takes symbols, refs, imports, edges, projects and modules), so the
/// pass below fills everything again.
fn drop_outdated_code(conn: &Connection) -> Result<(), EngineError> {
    let current = CODE_GRAPH_VERSION.to_string();
    if meta_value(conn, "code_graph_version")?.as_deref() == Some(current.as_str()) {
        return Ok(());
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM symbols_fts", [])?;
    tx.execute("DELETE FROM buckets", [])?;
    tx.execute("DELETE FROM files WHERE lang IS NOT NULL", [])?;
    set_meta(&tx, "code_graph_version", &current)?;
    // The next pass is a full fill, so it must not be taken for a finished one.
    set_meta(&tx, "code_building", "1")?;
    tx.commit()?;
    Ok(())
}

/// Brings the code rows in line with the tree. Returns how many files had
/// their graph rows replaced or dropped. With `stop_after`, stops once that
/// many batches are written and leaves `code_building` set (tests use it to
/// interrupt a fill).
pub(crate) fn refresh_code(
    repo: &Repo,
    conn: &Connection,
    batch_files: usize,
    stop_after: Option<usize>,
) -> Result<usize, EngineError> {
    drop_outdated_code(conn)?;
    let cands = candidates(repo);
    sync_project_dirs(conn, &project_dirs(repo, &cands))?;
    let known = known_code_files(conn)?;
    let on_disk: BTreeSet<&str> = cands.iter().map(|c| c.path.as_str()).collect();
    let mut stale: Vec<&Candidate> = cands
        .iter()
        .filter(|c| {
            known
                .get(&c.path)
                .is_none_or(|k| k.mtime_ns != c.mtime_ns || k.size != c.size)
        })
        .collect();
    let mut gone: Vec<(&str, i64)> = known
        .iter()
        .filter(|(path, _)| !on_disk.contains(path.as_str()))
        .map(|(path, k)| (path.as_str(), k.id))
        .collect();
    gone.sort();

    let resuming = meta_value(conn, "code_building")?.as_deref() == Some("1");
    if stale.is_empty() && gone.is_empty() && !resuming {
        return Ok(0);
    }

    if !resuming {
        set_meta(conn, "code_building", "1")?;
    }
    set_meta(conn, "code_total", &cands.len().to_string())?;

    let mut interrupted = false;
    if let Some(batches) = stop_after {
        let limit = batches * batch_files;
        interrupted = stale.len() > limit || gone.len() > limit;
        stale.truncate(limit);
        gone.truncate(limit);
    }

    let mut pass = Pass::default();
    for chunk in stale.chunks(batch_files.max(1)) {
        let parsed: Vec<Option<(String, Parsed)>> = chunk.par_iter().map(|c| parse(c, known.get(&c.path))).collect();
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        for (cand, item) in chunk.iter().zip(parsed) {
            if let Some((hash, parsed)) = item {
                write_file(&tx, cand, &hash, parsed, &mut pass)?;
            }
        }
        tx.commit()?;
    }
    in_batches(conn, gone, batch_files, |tx, batch| {
        for (_, id) in batch {
            let old = forget(tx, *id, &mut pass)?;
            pass.names.extend(old.names_changed(&Shape::default()).cloned());
            pass.replaced += 1;
        }
        Ok(())
    })?;
    if interrupted {
        return Ok(pass.replaced);
    }

    finish(conn, batch_files, &pass, resuming)?;
    Ok(pass.replaced)
}

/// Writes one file's rows. A row whose mtime and size already match was
/// written by another process in the meantime and is left alone.
fn write_file(
    tx: &Transaction,
    cand: &Candidate,
    hash: &str,
    parsed: Parsed,
    pass: &mut Pass,
) -> Result<(), EngineError> {
    let row: Option<(i64, i64, i64)> = tx
        .query_row(
            "SELECT id, mtime_ns, size FROM files WHERE path = ?1",
            [&cand.path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((_, mtime_ns, size)) = row
        && mtime_ns == cand.mtime_ns
        && size == cand.size
    {
        return Ok(());
    }
    if matches!(parsed, Parsed::Same) {
        tx.execute(
            "UPDATE files SET mtime_ns = ?1, size = ?2 WHERE path = ?3",
            params![cand.mtime_ns, cand.size, cand.path],
        )?;
        return Ok(());
    }
    let old = match row {
        Some((id, _, _)) => forget(tx, id, pass)?,
        None => Shape::default(),
    };

    let lang = match &parsed {
        Parsed::Code(facts) => facts.lang.as_str(),
        Parsed::Scip(language, _) => language.as_str(),
        _ => PROJECT_LANG,
    };
    tx.execute(
        "INSERT INTO files(path, hash, mtime_ns, size, item_id, lang) VALUES (?1, ?2, ?3, ?4, '', ?5)",
        params![cand.path, hash, cand.mtime_ns, cand.size, lang],
    )?;
    let file_id = tx.last_insert_rowid();
    pass.changed.insert(file_id);
    pass.replaced += 1;
    match parsed {
        Parsed::Code(facts) => {
            insert_code(tx, file_id, &facts)?;
            pass.names.extend(old.names_changed(&Shape::of(&facts)).cloned());
            // A new Dart file may be the target of an import that found nothing before, and a
            // file with no declarations (a barrel of exports) shows up in no name.
            if row.is_none() && facts.lang == Lang::Dart {
                pass.names.insert(uri_stem(&cand.path).to_string());
            }
            // What a library exports reaches every file behind its importers, and the `part`s of
            // a library share one scope: a file that wires libraries together moves the names of
            // files that have no edge to it.
            if facts.lang == Lang::Dart && facts.refs.iter().any(wires_dart_libraries) {
                pass.dirty_langs.insert(Lang::Dart);
            }
            if facts.imports.iter().any(|import| import.global) {
                pass.dirty_langs.insert(facts.lang);
            }
        }
        Parsed::Scip(language, file) => insert_scip_file(tx, file_id, &language, &file)?,
        Parsed::Project(facts) => {
            insert_project(tx, file_id, &facts)?;
            pass.projects_dirty = true;
            pass.dirty_langs.extend(governed_langs(facts.kind));
        }
        Parsed::Same => {}
    }
    Ok(())
}

/// The one file symbol of a `scip_indexes` file; its declarations come from the index.
fn insert_scip_file(tx: &Transaction, file_id: i64, language: &str, file: &Decl) -> Result<(), EngineError> {
    tx.execute(
        "INSERT INTO symbols(file_id, qname, name, kind, parent_id, signature, doc, line_start, line_end, lang, project) \
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, NULL, ?6, ?7, ?8, NULL)",
        params![
            file_id,
            file.qname,
            file.name,
            file.kind.as_str(),
            file.signature,
            file.line_start,
            file.line_end,
            language
        ],
    )?;
    tx.execute(
        "INSERT INTO symbols_fts(rowid, name, qname, signature, doc) VALUES (?1, ?2, ?3, ?4, NULL)",
        params![tx.last_insert_rowid(), file.name, file.qname, file.signature],
    )?;
    Ok(())
}

fn insert_code(tx: &Transaction, file_id: i64, facts: &FileFacts) -> Result<(), EngineError> {
    let mut symbol = tx.prepare_cached(
        "INSERT INTO symbols(file_id, qname, name, kind, parent_id, signature, doc, line_start, line_end, lang, project) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
    )?;
    let mut fts =
        tx.prepare_cached("INSERT INTO symbols_fts(rowid, name, qname, signature, doc) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    let mut ids: Vec<i64> = Vec::with_capacity(facts.decls.len());
    let mut late_parents: Vec<(usize, usize)> = Vec::new();
    for (index, decl) in facts.decls.iter().enumerate() {
        let parent = match decl.parent {
            Some(p) if p < index => Some(ids[p]),
            Some(p) if p < facts.decls.len() => {
                late_parents.push((index, p));
                None
            }
            _ => None,
        };
        symbol.execute(params![
            file_id,
            decl.qname,
            decl.name,
            decl.kind.as_str(),
            parent,
            decl.signature,
            decl.doc,
            decl.line_start,
            decl.line_end,
            facts.lang.as_str(),
        ])?;
        let id = tx.last_insert_rowid();
        fts.execute(params![id, decl.name, decl.qname, decl.signature, decl.doc])?;
        ids.push(id);
    }
    for (index, parent) in late_parents {
        tx.execute(
            "UPDATE symbols SET parent_id = ?1 WHERE id = ?2",
            params![ids[parent], ids[index]],
        )?;
    }

    let mut site = tx.prepare_cached(
        "INSERT INTO refs(file_id, src_symbol_id, name, qualifier, kind, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for r in &facts.refs {
        let scope = r.scope.and_then(|s| ids.get(s)).copied();
        site.execute(params![file_id, scope, r.name, r.qualifier, r.kind.as_str(), r.line])?;
    }
    let mut import = tx.prepare_cached(
        "INSERT INTO imports(file_id, path, alias, is_glob, is_static, is_global, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for i in &facts.imports {
        import.execute(params![file_id, i.path, i.alias, i.glob, i.is_static, i.global, i.line])?;
    }
    let mut warning = tx.prepare_cached("INSERT INTO parse_warnings(file_id, line, message) VALUES (?1, ?2, ?3)")?;
    for w in &facts.warnings {
        warning.execute(params![file_id, w.line, w.message])?;
    }
    Ok(())
}

/// How many grammar failures `syntax_problems` lists by position.
const SYNTAX_LISTED: usize = 5;

/// One `syntax` warning for the whole tree when any file parsed with errors: the count of
/// failures and files, and the first few as `path:line`. The warning sits at the first one.
pub(crate) fn syntax_problems(conn: &Connection, out: &mut Vec<Problem>) -> Result<(), EngineError> {
    let (total, files): (i64, i64) = conn.query_row(
        "SELECT count(*), count(DISTINCT file_id) FROM parse_warnings",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if total == 0 {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "SELECT f.path, w.line FROM parse_warnings w JOIN files f ON f.id = w.file_id \
         ORDER BY f.path, w.line, w.rowid LIMIT ?1",
    )?;
    let first: Vec<(String, i64)> = stmt
        .query_map([SYNTAX_LISTED as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let listed: Vec<String> = first.iter().map(|(path, line)| format!("{path}:{line}")).collect();
    let more = if total as usize > first.len() {
        format!(
            ", and {} more; `stone check --syntax` lists them all",
            total as usize - first.len()
        )
    } else {
        String::new()
    };
    let (path, line) = &first[0];
    out.push(Problem::warning(
        path,
        usize::try_from(*line).unwrap_or(1),
        "syntax",
        format!(
            "{total} parse {} in {files} {}; fix the syntax or the code graph misses what is around it. First: {}{more}",
            if total == 1 { "error" } else { "errors" },
            if files == 1 { "file" } else { "files" },
            listed.join(", ")
        ),
    ));
    Ok(())
}

/// Every grammar failure as its own warning, by path and line, for `stone check --syntax`.
pub(crate) fn syntax_sites(conn: &Connection) -> Result<Vec<Problem>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT f.path, w.line, w.message FROM parse_warnings w JOIN files f ON f.id = w.file_id \
         ORDER BY f.path, w.line, w.rowid",
    )?;
    let sites = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .map(|site| {
            site.map(|(path, line, message)| {
                Problem::warning(&path, usize::try_from(line).unwrap_or(1), "syntax", message)
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(sites)
}

fn insert_project(tx: &Transaction, file_id: i64, facts: &ProjectFacts) -> Result<(), EngineError> {
    tx.execute(
        "INSERT INTO projects(file_id, name, root) VALUES (?1, ?2, ?3)",
        params![file_id, facts.name, facts.root],
    )?;
    let mut item = tx.prepare_cached("INSERT INTO project_items(file_id, kind, name, path) VALUES (?1, ?2, ?3, ?4)")?;
    for r in &facts.references {
        item.execute(params![file_id, "reference", r.name, r.path])?;
    }
    for package in &facts.packages {
        item.execute(params![file_id, "package", package, ""])?;
    }
    for root in &facts.roots {
        item.execute(params![file_id, "root", "", root])?;
    }
    if let Some(base) = &facts.base_url {
        item.execute(params![file_id, "base_url", "", base])?;
    }
    for alias in &facts.aliases {
        item.execute(params![file_id, "alias", alias.pattern, alias.target])?;
    }
    // The project is a node of the graph: `imports` edges run between these.
    tx.execute(
        "INSERT INTO symbols(file_id, qname, name, kind, signature, line_start, line_end, lang, project) \
         VALUES (?1, ?2, ?2, 'project', ?3, 1, 1, ?4, ?2)",
        params![file_id, facts.name, facts.kind.as_str(), kind_lang(facts.kind)],
    )?;
    tx.execute(
        "INSERT INTO symbols_fts(rowid, name, qname, signature, doc) VALUES (?1, ?2, ?2, ?3, NULL)",
        params![tx.last_insert_rowid(), facts.name, facts.kind.as_str()],
    )?;
    Ok(())
}

/// Drops one file's rows (edges into it go with its symbols) and notes what
/// the edges half has to look at. Returns the file's old shape.
fn forget(tx: &Transaction, id: i64, pass: &mut Pass) -> Result<Shape, EngineError> {
    let Some((path, lang)): Option<(String, String)> = tx
        .query_row("SELECT path, lang FROM files WHERE id = ?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?
    else {
        return Ok(Shape::default());
    };
    let mut shape = Shape::default();
    {
        let mut symbols = tx.prepare_cached(
            "SELECT s.qname, s.name, s.kind, ifnull(p.qname, '') FROM symbols s \
             LEFT JOIN symbols p ON p.id = s.parent_id WHERE s.file_id = ?1 AND s.kind NOT IN ('file', 'project')",
        )?;
        let rows = symbols.query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
        for row in rows {
            *shape.symbols.entry(row?).or_default() += 1;
        }
        let mut ties = tx.prepare_cached(
            "SELECT r.kind, r.name, ifnull(r.qualifier, ''), s.name FROM refs r JOIN symbols s ON s.id = r.src_symbol_id \
             WHERE r.file_id = ?1 AND r.kind NOT IN ('calls', 'imports')",
        )?;
        let rows = ties.query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
        for row in rows {
            shape.ties.insert(row?);
        }
        let mut inbound = tx.prepare_cached(
            "SELECT DISTINCT e.file_id FROM edges e JOIN symbols s ON s.id = e.dst_id WHERE s.file_id = ?1 AND e.file_id <> ?1",
        )?;
        for file in inbound.query_map([id], |row| row.get::<_, i64>(0))? {
            pass.inbound.insert(file?);
        }
    }
    if lang == PROJECT_LANG {
        pass.projects_dirty = true;
        if let Some(kind) = ProjectKind::of(&path) {
            pass.dirty_langs.extend(governed_langs(kind));
        }
    } else if let Some(lang) = lang_from(&lang) {
        let global = tx
            .query_row(
                "SELECT 1 FROM imports WHERE file_id = ?1 AND is_global = 1 LIMIT 1",
                [id],
                |_| Ok(()),
            )
            .optional()?;
        if global.is_some() {
            pass.dirty_langs.insert(lang);
        }
        // A Dart file that exported or had parts (see `wires_dart_libraries`).
        if lang == Lang::Dart {
            let wired = tx
                .query_row(
                    "SELECT 1 FROM refs WHERE file_id = ?1 AND kind = 'imports' \
                     AND (qualifier LIKE 'export%' OR qualifier LIKE 'part%') LIMIT 1",
                    [id],
                    |_| Ok(()),
                )
                .optional()?;
            if wired.is_some() {
                pass.dirty_langs.insert(lang);
            }
        }
    }
    tx.execute(
        "DELETE FROM symbols_fts WHERE rowid IN (SELECT id FROM symbols WHERE file_id = ?1)",
        [id],
    )?;
    tx.execute("DELETE FROM files WHERE id = ?1", [id])?;
    Ok(shape)
}

// ---------------------------------------------------------------- world

/// The resolver world rebuilt from rows, with the row ids its indices map to.
struct Loaded {
    world: World,
    file_ids: Vec<i64>,
    /// Per world file, the symbol id of each of its decls.
    sym_ids: Vec<Vec<i64>>,
    project_file_ids: Vec<i64>,
    project_sym_ids: Vec<Option<i64>>,
}

impl Loaded {
    fn node_id(&self, node: &Node) -> Option<i64> {
        match node {
            Node::Decl(sym) => self.sym_ids.get(sym.file)?.get(sym.decl).copied(),
            Node::Project(index) => self.project_sym_ids.get(*index).copied().flatten(),
            Node::Bucket(_) => None,
        }
    }

    fn node_file(&self, node: &Node) -> Option<i64> {
        match node {
            Node::Decl(sym) => self.file_ids.get(sym.file).copied(),
            Node::Project(index) => self.project_file_ids.get(*index).copied(),
            Node::Bucket(_) => None,
        }
    }
}

struct RawFile {
    id: i64,
    facts: FileFacts,
    sym_ids: Vec<i64>,
    parents: Vec<Option<i64>>,
    index_of: HashMap<i64, usize>,
}

fn load_world(conn: &Connection) -> Result<Loaded, EngineError> {
    let mut raws: Vec<RawFile> = Vec::new();
    let mut at: HashMap<i64, usize> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, path, lang FROM files WHERE lang IS NOT NULL AND lang <> 'project'")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (id, path, lang): (i64, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
            let Some(lang) = lang_from(&lang) else {
                continue;
            };
            at.insert(id, raws.len());
            raws.push(RawFile {
                id,
                facts: FileFacts::new(lang, &path),
                sym_ids: Vec::new(),
                parents: Vec::new(),
                index_of: HashMap::new(),
            });
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT s.file_id, s.id, s.qname, s.name, s.kind, s.parent_id, s.signature, s.doc, s.line_start, s.line_end \
             FROM symbols s JOIN files f ON f.id = s.file_id WHERE f.lang <> 'project' ORDER BY s.file_id, s.id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let Some(&slot) = at.get(&row.get::<_, i64>(0)?) else {
                continue;
            };
            let raw = &mut raws[slot];
            let id: i64 = row.get(1)?;
            raw.index_of.insert(id, raw.facts.decls.len());
            raw.sym_ids.push(id);
            raw.parents.push(row.get(5)?);
            raw.facts.decls.push(Decl {
                qname: row.get(2)?,
                name: row.get(3)?,
                kind: decl_kind(&row.get::<_, String>(4)?),
                parent: None,
                signature: row.get(6)?,
                doc: row.get(7)?,
                line_start: row.get(8)?,
                line_end: row.get(9)?,
            });
        }
    }
    for raw in &mut raws {
        for (index, parent) in raw.parents.iter().enumerate() {
            raw.facts.decls[index].parent = parent.and_then(|p| raw.index_of.get(&p).copied());
        }
    }
    {
        let mut stmt =
            conn.prepare("SELECT file_id, src_symbol_id, name, qualifier, kind, line FROM refs ORDER BY file_id, id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let Some(&slot) = at.get(&row.get::<_, i64>(0)?) else {
                continue;
            };
            let raw = &mut raws[slot];
            let scope = row
                .get::<_, Option<i64>>(1)?
                .and_then(|s| raw.index_of.get(&s).copied());
            raw.facts.refs.push(RefSite {
                name: row.get(2)?,
                qualifier: row.get(3)?,
                kind: ref_kind(&row.get::<_, String>(4)?),
                line: row.get(5)?,
                scope,
            });
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT file_id, path, alias, is_glob, is_static, is_global, line FROM imports ORDER BY file_id, rowid",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let Some(&slot) = at.get(&row.get::<_, i64>(0)?) else {
                continue;
            };
            raws[slot].facts.imports.push(ImportDecl {
                path: row.get(1)?,
                alias: row.get(2)?,
                glob: row.get(3)?,
                is_static: row.get(4)?,
                global: row.get(5)?,
                line: row.get(6)?,
            });
        }
    }

    struct RawProject {
        file_id: i64,
        sym_id: Option<i64>,
        facts: ProjectFacts,
    }
    let mut projects: Vec<RawProject> = Vec::new();
    let mut project_at: HashMap<i64, usize> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT f.id, f.path, p.name, p.root FROM projects p JOIN files f ON f.id = p.file_id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (file_id, path): (i64, String) = (row.get(0)?, row.get(1)?);
            let Some(kind) = ProjectKind::of(&path) else {
                continue;
            };
            project_at.insert(file_id, projects.len());
            projects.push(RawProject {
                file_id,
                sym_id: None,
                facts: ProjectFacts {
                    path,
                    kind,
                    name: row.get(2)?,
                    root: row.get(3)?,
                    references: Vec::new(),
                    packages: Vec::new(),
                    roots: Vec::new(),
                    base_url: None,
                    aliases: Vec::new(),
                },
            });
        }
    }
    {
        let mut stmt = conn.prepare("SELECT file_id, kind, name, path FROM project_items ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let Some(&slot) = project_at.get(&row.get::<_, i64>(0)?) else {
                continue;
            };
            let facts = &mut projects[slot].facts;
            let (name, path): (String, String) = (row.get(2)?, row.get(3)?);
            match row.get::<_, String>(1)?.as_str() {
                "reference" => facts.references.push(ProjectReference { name, path }),
                "package" => facts.packages.push(name),
                "base_url" => facts.base_url = Some(path),
                "alias" => facts.aliases.push(PathAlias {
                    pattern: name,
                    target: path,
                }),
                _ => facts.roots.push(path),
            }
        }
    }
    {
        let mut stmt = conn.prepare("SELECT file_id, id FROM symbols WHERE kind = 'project'")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if let Some(&slot) = project_at.get(&row.get::<_, i64>(0)?) {
                projects[slot].sym_id = Some(row.get(1)?);
            }
        }
    }

    let mut side: HashMap<String, (i64, Vec<i64>)> = HashMap::new();
    let mut files = Vec::with_capacity(raws.len());
    for raw in raws {
        side.insert(raw.facts.path.clone(), (raw.id, raw.sym_ids));
        files.push(raw.facts);
    }
    let mut project_side: HashMap<String, (i64, Option<i64>)> = HashMap::new();
    let mut project_facts_list = Vec::with_capacity(projects.len());
    for project in projects {
        project_side.insert(project.facts.path.clone(), (project.file_id, project.sym_id));
        project_facts_list.push(project.facts);
    }

    let world = World::new(files, project_facts_list);
    let mut file_ids = Vec::new();
    let mut sym_ids = Vec::new();
    for file in world.files() {
        let (id, syms) = side.remove(&file.path).unwrap_or_default();
        file_ids.push(id);
        sym_ids.push(syms);
    }
    let mut project_file_ids = Vec::new();
    let mut project_sym_ids = Vec::new();
    for project in world.projects() {
        let (id, sym) = project_side.get(&project.path).copied().unwrap_or_default();
        project_file_ids.push(id);
        project_sym_ids.push(sym);
    }
    Ok(Loaded {
        world,
        file_ids,
        sym_ids,
        project_file_ids,
        project_sym_ids,
    })
}

// ---------------------------------------------------------------- edges

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeRow {
    src_id: i64,
    dst_id: Option<i64>,
    dst_bucket: Option<i64>,
    dst_name: String,
    kind: String,
    exact: bool,
    line: i64,
}

fn last_segment(path: &str) -> &str {
    path.rsplit(['.', ':', '/']).next().unwrap_or(path)
}

/// Whether a Dart directive joins libraries (`export`, `part`, `part of`) rather than only
/// importing from one.
fn wires_dart_libraries(site: &RefSite) -> bool {
    site.kind == RefKind::Imports
        && site
            .qualifier
            .as_deref()
            .is_some_and(|q| q.starts_with("export") || q.starts_with("part"))
}

/// The file name of a path or URI without its extension: `../a/user.dart` is `user`.
fn uri_stem(uri: &str) -> &str {
    let name = uri.rsplit('/').next().unwrap_or(uri);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// World indices of the files that name any of `names` in a reference, a
/// qualifier or an import.
fn dependents(world: &World, names: &BTreeSet<String>) -> BTreeSet<usize> {
    if names.is_empty() {
        return BTreeSet::new();
    }
    world
        .files()
        .iter()
        .enumerate()
        .filter(|(_, file)| {
            file.refs.iter().any(|site| {
                names.contains(&site.name)
                    || site
                        .qualifier
                        .as_deref()
                        .is_some_and(|q| q.split(['.', ':']).any(|part| names.contains(part)))
                    // A Dart directive names a file; a new file's stem is in `names` (see `write_file`).
                    || (site.kind == RefKind::Imports && file.lang == Lang::Dart && names.contains(uri_stem(&site.name)))
            }) || file
                .imports
                .iter()
                .any(|import| names.contains(last_segment(&import.path)))
        })
        .map(|(index, _)| index)
        .collect()
}

/// World indices of the files with a base-list entry that names one of `types`
/// or sits in a type of that name.
fn heritage_dependents(world: &World, types: &BTreeSet<String>) -> BTreeSet<usize> {
    if types.is_empty() {
        return BTreeSet::new();
    }
    world
        .files()
        .iter()
        .enumerate()
        .filter(|(_, file)| {
            file.refs.iter().any(|site| {
                matches!(site.kind, RefKind::Inherits | RefKind::Implements)
                    && (types.contains(&site.name)
                        || site
                            .scope
                            .and_then(|scope| file.decls.get(scope))
                            .is_some_and(|decl| types.contains(&decl.name)))
            })
        })
        .map(|(index, _)| index)
        .collect()
}

fn finish(conn: &Connection, batch_files: usize, pass: &Pass, full: bool) -> Result<(), EngineError> {
    let loaded = load_world(conn)?;
    let world = &loaded.world;

    let mut affected: BTreeSet<usize> = BTreeSet::new();
    if full {
        affected.extend(0..world.files().len());
    } else {
        for (index, id) in loaded.file_ids.iter().enumerate() {
            if pass.changed.contains(id)
                || pass.inbound.contains(id)
                || pass.dirty_langs.contains(&world.files()[index].lang)
            {
                affected.insert(index);
            }
        }
        affected.extend(dependents(world, &pass.names));
        // A dispatch edge sits in the file of the base-list entry that causes it,
        // but depends on the members of both types: when either side's file was
        // replaced, the files that list those types as bases are looked at again.
        let mut types = pass.names.clone();
        for (index, id) in loaded.file_ids.iter().enumerate() {
            if pass.changed.contains(id) {
                types.extend(
                    world.files()[index]
                        .decls
                        .iter()
                        .filter(|decl| decl.kind == DeclKind::Type)
                        .map(|decl| decl.name.clone()),
                );
            }
        }
        affected.extend(heritage_dependents(world, &types));
    }

    let mut by_lang: BTreeMap<Lang, Vec<usize>> = BTreeMap::new();
    for &index in &affected {
        by_lang.entry(world.files()[index].lang).or_default().push(index);
    }
    let groups: Vec<(Lang, Vec<usize>)> = by_lang.into_iter().collect();
    let mut edges: Vec<Edge> = groups
        .par_iter()
        .flat_map(|(lang, files)| resolver_for(*lang).resolve_files(world, files))
        .collect();
    let with_projects = full || pass.projects_dirty;
    if with_projects {
        for lang in [Lang::CSharp, Lang::Rust, Lang::TypeScript, Lang::Python] {
            edges.extend(resolver_for(lang).resolve_projects(world));
        }
    }

    // A name guess never crosses into another project's repo; only an exact edge does (a manifest
    // reference resolved it).
    let dirs = stored_project_dirs(conn)?;
    if !dirs.is_empty() {
        edges.retain(|edge| {
            let (Node::Decl(src), Some(Node::Decl(dst))) = (&edge.src, &edge.dst) else {
                return true;
            };
            edge.exact
                || project_dir(&dirs, &world.files()[src.file].path)
                    == project_dir(&dirs, &world.files()[dst.file].path)
        });
    }

    // Dispatch is derived from the base edges just resolved, so it covers exactly the affected files.
    let dispatched = dispatch(world, &edges);
    let mut used = bucket_uses(world, &dirs, &edges);
    used.extend(held_buckets(conn, &loaded, pass)?);
    let bucket_ids = ensure_buckets(conn, &used)?;

    // One entry per file whose edges are compared: every affected file, and
    // every project file when project edges were recomputed.
    let mut work: BTreeMap<i64, (Vec<EdgeRow>, Option<Option<String>>)> = BTreeMap::new();
    for &index in &affected {
        let file = &world.files()[index];
        let project = governing_project(world, file.lang, &file.path);
        work.insert(loaded.file_ids[index], (Vec::new(), Some(project)));
    }
    if with_projects {
        for &id in &loaded.project_file_ids {
            work.entry(id).or_default();
        }
    }
    for edge in &edges {
        let (Some(file_id), Some(src_id)) = (loaded.node_file(&edge.src), loaded.node_id(&edge.src)) else {
            continue;
        };
        let (dst_id, bucket) = match &edge.dst {
            None => (None, None),
            Some(Node::Decl(sym)) => (loaded.node_id(&Node::Decl(*sym)), None),
            Some(Node::Project(index)) => (loaded.node_id(&Node::Project(*index)), None),
            Some(Node::Bucket(bucket)) => {
                let Node::Decl(src) = &edge.src else {
                    continue;
                };
                let file = &world.files()[src.file];
                (None, Some((file.lang, tagged(&dirs, &file.path, bucket))))
            }
        };
        work.entry(file_id).or_default().0.push(EdgeRow {
            src_id,
            dst_id,
            dst_bucket: bucket.as_ref().map(|key| bucket_ids[key]),
            dst_name: edge.dst_name.clone(),
            kind: edge.kind.as_str().to_string(),
            exact: edge.exact,
            line: i64::from(edge.line),
        });
    }

    for (owner, edge) in &dispatched {
        let (Some(file_id), Some(src_id), Some(dst_id)) = (
            loaded.file_ids.get(*owner).copied(),
            loaded.node_id(&edge.src),
            edge.dst.as_ref().and_then(|dst| loaded.node_id(dst)),
        ) else {
            continue;
        };
        work.entry(file_id).or_default().0.push(EdgeRow {
            src_id,
            dst_id: Some(dst_id),
            dst_bucket: None,
            dst_name: edge.dst_name.clone(),
            kind: edge.kind.as_str().to_string(),
            exact: edge.exact,
            line: i64::from(edge.line),
        });
    }

    let mut changed = pass.replaced > 0;
    in_batches(conn, work.into_iter().collect(), batch_files, |tx, batch| {
        for (file_id, (rows, project)) in batch {
            let mut rows = rows.clone();
            rows.sort();
            if sync_edges(tx, *file_id, &rows)? {
                changed = true;
            }
            if let Some(project) = project {
                let moved = tx.execute(
                    "UPDATE symbols SET project = ?1 WHERE file_id = ?2 AND kind <> 'project' AND project IS NOT ?1",
                    params![project, file_id],
                )?;
                changed |= moved > 0;
            }
        }
        Ok(())
    })?;
    if sync_buckets(conn, world, &loaded, &used, &bucket_ids)? {
        changed = true;
    }

    if changed {
        let version: i64 = meta_value(conn, "graph_version")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        set_meta(conn, "graph_version", &(version + 1).to_string())?;
    }
    set_meta(conn, "code_building", "0")?;
    Ok(())
}

/// Replaces the edges of `file_id` with `rows` (sorted) when they differ.
fn sync_edges(tx: &Transaction, file_id: i64, rows: &[EdgeRow]) -> Result<bool, EngineError> {
    let mut old: Vec<EdgeRow> = {
        let mut stmt = tx.prepare_cached(
            "SELECT src_id, dst_id, dst_bucket, dst_name, kind, exact, line FROM edges WHERE file_id = ?1",
        )?;
        stmt.query_map([file_id], |row| {
            Ok(EdgeRow {
                src_id: row.get(0)?,
                dst_id: row.get(1)?,
                dst_bucket: row.get(2)?,
                dst_name: row.get(3)?,
                kind: row.get(4)?,
                exact: row.get::<_, i64>(5)? != 0,
                line: row.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?
    };
    old.sort();
    if old == rows {
        return Ok(false);
    }
    tx.execute("DELETE FROM edges WHERE file_id = ?1", [file_id])?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO edges(src_id, dst_id, dst_bucket, dst_name, kind, exact, file_id, line) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for row in rows {
        insert.execute(params![
            row.src_id,
            row.dst_id,
            row.dst_bucket,
            row.dst_name,
            row.kind,
            row.exact,
            file_id,
            row.line
        ])?;
    }
    Ok(true)
}

// ---------------------------------------------------------------- buckets

/// The buckets the resolved edges point at, by language: a bucket's members
/// are computed by that language's resolver.
fn bucket_uses(world: &World, dirs: &[String], edges: &[Edge]) -> BTreeMap<(Lang, Bucket), ()> {
    let mut used = BTreeMap::new();
    for edge in edges {
        if let (Node::Decl(src), Some(Node::Bucket(bucket))) = (&edge.src, &edge.dst) {
            let file = &world.files()[src.file];
            used.insert((file.lang, tagged(dirs, &file.path, bucket)), ());
        }
    }
    used
}

/// Separates the scope of a bucket from the project folder it belongs to.
const FOLDER_MARK: char = '\u{1f}';

/// The project folder of `path`: the longest of `dirs` holding it, `""` when none does.
pub(crate) fn project_dir<'a>(dirs: &'a [String], path: &str) -> &'a str {
    dirs.iter()
        .filter(|dir| contains(dir, path))
        .max_by_key(|dir| dir.len())
        .map_or("", String::as_str)
}

/// A name bucket of a file in a project folder is that folder's own: the same name in another
/// repo is another bucket, so a guess by name never reaches across repos. A root with no project
/// folders keeps the bucket as it is.
fn tagged(dirs: &[String], path: &str, bucket: &Bucket) -> Bucket {
    match project_dir(dirs, path) {
        "" => bucket.clone(),
        dir => Bucket::new(format!("{dir}{FOLDER_MARK}{}", bucket.scope), bucket.name.clone()),
    }
}

/// The bucket as the resolver knows it, and the project folder it is tied to (`""` for none).
fn untagged(bucket: &Bucket) -> (Bucket, &str) {
    match bucket.scope.split_once(FOLDER_MARK) {
        Some((dir, scope)) => (Bucket::new(scope, bucket.name.clone()), dir),
        None => (bucket.clone(), ""),
    }
}

/// Existing buckets named like a declaration of a replaced file. Replacing a
/// file drops its symbols from the buckets that held them, so those buckets
/// are filled again even when no site pointing at them was re-resolved.
fn held_buckets(conn: &Connection, loaded: &Loaded, pass: &Pass) -> Result<BTreeMap<(Lang, Bucket), ()>, EngineError> {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for (index, id) in loaded.file_ids.iter().enumerate() {
        if pass.changed.contains(id) {
            names.extend(loaded.world.files()[index].decls.iter().map(|decl| decl.name.as_str()));
        }
    }
    let mut held = BTreeMap::new();
    if names.is_empty() {
        return Ok(held);
    }
    let mut stmt = conn.prepare("SELECT lang, scope, name FROM buckets")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (lang, scope, name) = row?;
        if let Some(lang) = lang_from(&lang)
            && names.contains(name.as_str())
        {
            held.insert((lang, Bucket::new(scope, name)), ());
        }
    }
    Ok(held)
}

/// Writes the bucket rows the edges need and returns their ids. Ids of rows
/// that were already there are reused, so a bucket keeps its identity.
fn ensure_buckets(
    conn: &Connection,
    used: &BTreeMap<(Lang, Bucket), ()>,
) -> Result<BTreeMap<(Lang, Bucket), i64>, EngineError> {
    let mut ids = BTreeMap::new();
    if used.is_empty() {
        return Ok(ids);
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    {
        let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO buckets(lang, scope, name) VALUES (?1, ?2, ?3)")?;
        let mut select = tx.prepare_cached("SELECT id FROM buckets WHERE lang = ?1 AND scope = ?2 AND name = ?3")?;
        for (lang, bucket) in used.keys() {
            insert.execute(params![lang.as_str(), bucket.scope, bucket.name])?;
            let id: i64 = select.query_row(params![lang.as_str(), bucket.scope, bucket.name], |row| row.get(0))?;
            ids.insert((*lang, bucket.clone()), id);
        }
    }
    tx.commit()?;
    Ok(ids)
}

/// Rewrites the member rows of every bucket this pass used, and drops buckets
/// no edge points at any more. Only a changed membership writes anything.
fn sync_buckets(
    conn: &Connection,
    world: &World,
    loaded: &Loaded,
    used: &BTreeMap<(Lang, Bucket), ()>,
    ids: &BTreeMap<(Lang, Bucket), i64>,
) -> Result<bool, EngineError> {
    let mut by_lang: BTreeMap<Lang, Vec<(&Bucket, i64)>> = BTreeMap::new();
    for (lang, bucket) in used.keys() {
        by_lang
            .entry(*lang)
            .or_default()
            .push((bucket, ids[&(*lang, bucket.clone())]));
    }
    let dirs = stored_project_dirs(conn)?;
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut changed = false;
    for (lang, list) in by_lang {
        let plain: Vec<(Bucket, &str)> = list.iter().map(|(bucket, _)| untagged(bucket)).collect();
        let buckets: Vec<Bucket> = plain.iter().map(|(bucket, _)| bucket.clone()).collect();
        let members = resolver_for(lang).bucket_members(world, &buckets);
        for (((_, id), (_, dir)), found) in list.iter().zip(&plain).zip(members) {
            let mut symbols: Vec<i64> = found
                .iter()
                .filter(|sym| project_dir(&dirs, &world.files()[sym.file].path) == *dir)
                .filter_map(|sym| loaded.node_id(&Node::Decl(*sym)))
                .collect();
            symbols.sort_unstable();
            symbols.dedup();
            changed |= sync_bucket_members(&tx, *id, &symbols)?;
        }
    }
    let removed = tx.execute(
        "DELETE FROM buckets WHERE id NOT IN (SELECT dst_bucket FROM edges WHERE dst_bucket IS NOT NULL)",
        [],
    )?;
    changed |= removed > 0;
    tx.commit()?;
    Ok(changed)
}

fn sync_bucket_members(tx: &Transaction, bucket_id: i64, symbols: &[i64]) -> Result<bool, EngineError> {
    let old: Vec<i64> = {
        let mut stmt =
            tx.prepare_cached("SELECT symbol_id FROM bucket_members WHERE bucket_id = ?1 ORDER BY symbol_id")?;
        stmt.query_map([bucket_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?
    };
    if old == symbols {
        return Ok(false);
    }
    tx.execute("DELETE FROM bucket_members WHERE bucket_id = ?1", [bucket_id])?;
    let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO bucket_members(bucket_id, symbol_id) VALUES (?1, ?2)")?;
    for symbol in symbols {
        insert.execute(params![bucket_id, symbol])?;
    }
    Ok(true)
}

// ---------------------------------------------------------------- test support

/// Every graph row without row ids, one line each in a fixed order, so two
/// indexes can be compared. Used by tests; not a stable format.
#[doc(hidden)]
pub fn graph_dump(repo: &Repo) -> Result<String, EngineError> {
    let index = repo.index()?;
    let conn = index.conn();
    let mut out = String::new();
    let queries: [(&str, &str, usize); 8] = [
        (
            "S",
            "SELECT f.path, s.qname, s.kind, ifnull((SELECT p.qname FROM symbols p WHERE p.id = s.parent_id), ''), \
             s.signature, ifnull(s.doc, ''), s.line_start || '-' || s.line_end, s.lang, ifnull(s.project, '-') \
             FROM symbols s JOIN files f ON f.id = s.file_id ORDER BY f.path, s.id",
            9,
        ),
        (
            "R",
            "SELECT f.path, ifnull((SELECT s.qname FROM symbols s WHERE s.id = r.src_symbol_id), ''), r.name, \
             ifnull(r.qualifier, '-'), r.kind, r.line, '', '', '' FROM refs r JOIN files f ON f.id = r.file_id ORDER BY f.path, r.id",
            9,
        ),
        (
            "I",
            "SELECT f.path, i.path, ifnull(i.alias, '-'), i.is_glob, i.is_static, i.is_global, i.line, '', '' \
             FROM imports i JOIN files f ON f.id = i.file_id ORDER BY f.path, i.rowid",
            9,
        ),
        (
            "E",
            "SELECT f.path, s.qname, ifnull(d.qname, '-'), ifnull(df.path, '-'), e.dst_name, e.kind, e.exact, e.line, \
             ifnull((SELECT b.lang || ':' || b.scope || ':' || b.name FROM buckets b WHERE b.id = e.dst_bucket), '-') \
             FROM edges e JOIN files f ON f.id = e.file_id JOIN symbols s ON s.id = e.src_id \
             LEFT JOIN symbols d ON d.id = e.dst_id LEFT JOIN files df ON df.id = d.file_id \
             ORDER BY f.path, s.qname, e.line, e.kind, e.dst_name, ifnull(d.qname, ''), ifnull(df.path, ''), e.exact",
            9,
        ),
        (
            "B",
            "SELECT lang, scope, name, '', '', '', '', '', '' FROM buckets ORDER BY lang, scope, name",
            9,
        ),
        (
            "M",
            "SELECT b.lang, b.scope, b.name, df.path, d.qname, '', '', '', '' \
             FROM bucket_members m JOIN buckets b ON b.id = m.bucket_id \
             JOIN symbols d ON d.id = m.symbol_id JOIN files df ON df.id = d.file_id \
             ORDER BY b.lang, b.scope, b.name, df.path, d.qname",
            9,
        ),
        (
            "P",
            "SELECT f.path, p.name, p.root, ifnull((SELECT group_concat(i.kind || ':' || i.name || ':' || i.path, ',') \
             FROM project_items i WHERE i.file_id = f.id), ''), '', '', '', '', '' \
             FROM projects p JOIN files f ON f.id = p.file_id ORDER BY f.path",
            9,
        ),
        (
            "F",
            "SELECT path, lang, hash, '', '', '', '', '', '' FROM files WHERE lang IS NOT NULL ORDER BY path",
            9,
        ),
    ];
    for (tag, sql, columns) in queries {
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            out.push_str(tag);
            for column in 0..columns {
                let value: rusqlite::types::Value = row.get(column)?;
                let text = match value {
                    rusqlite::types::Value::Null => "-".to_string(),
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Real(n) => n.to_string(),
                    rusqlite::types::Value::Text(t) => t,
                    rusqlite::types::Value::Blob(_) => "blob".to_string(),
                };
                out.push('|');
                out.push_str(&text);
            }
            out.push('\n');
        }
    }
    let fts: i64 = conn.query_row("SELECT count(*) FROM symbols_fts", [], |row| row.get(0))?;
    let symbols: i64 = conn.query_row("SELECT count(*) FROM symbols", [], |row| row.get(0))?;
    out.push_str(&format!("fts={fts} symbols={symbols}\n"));
    Ok(out)
}
