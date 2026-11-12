//! Staleness: docs, references and arch items that nobody re-read after their code moved on.
//!
//! An item is stale when its `stale_after` day has passed, or when at least `stale_commits`
//! commits touched the code it describes after its reference day: the latest `verified.at`,
//! else `generated.at`. The code is the item's own `code` globs for arch items, else the `code`
//! globs of the components it lists.
//!
//! A doc that names code files is stale as soon as one commit changed one of them after the later
//! of its reference day and the day of its own last commit. Named files are `[[code:...]]` targets
//! and code spans that are the path of a file on disk (root-relative, or relative to the folder of
//! a plain doc). Plain docs (doc-root and project files) follow this rule only.
//!
//! Days are compared as days, never as times. Tasks, plans, milestones and decisions are never
//! stale. Without git only `stale_after` applies.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use carvestone_core::section::{code_span_ranges, lines};
use carvestone_core::{ItemType, body_links};
use carvestone_store::Entry;
use globset::GlobSet;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::check_links::{CodeLink, CodeResolver};
use crate::git_cache::index_with_history;
use crate::owner::{Element, arch_elements};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::timeline::{Date, parse_date};
use crate::why::names;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Stale {
    pub reason: String,
    /// Commits that touched the code after the reference day; 0 when `stale_after` alone decided.
    pub commits: u32,
}

/// Whether `stale` applies to this type at all.
pub(crate) fn can_be_stale(ty: Option<ItemType>) -> bool {
    matches!(
        ty,
        Some(ItemType::Doc | ItemType::Reference | ItemType::System | ItemType::Container | ItemType::Component)
    )
}

/// The day a stamp or `stale_after` text names: its first ten characters, `YYYY-MM-DD`.
pub(crate) fn day_of(text: &str) -> Option<Date> {
    let day = text.trim().get(..10)?;
    // `parse_date` also reads `5d`; a stamp is always an absolute day.
    if day.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    parse_date("day", day, Date::today()).ok()
}

/// The latest `verified.at` day, else the `generated.at` day, with the word the reason uses.
fn reference_day(entry: &Entry) -> Option<(Date, &'static str)> {
    let item = entry.item.as_ref()?;
    let verified = item.fields.verified.iter().filter_map(|s| day_of(&s.at)).max();
    if let Some(day) = verified {
        return Some((day, "verified"));
    }
    let generated = item.fields.generated.as_ref().and_then(|s| day_of(&s.at))?;
    Some((generated, "generated"))
}

/// One item the commit rule may apply to.
struct Candidate<'a> {
    id: String,
    since: Date,
    label: &'static str,
    globs: Vec<&'a GlobSet>,
}

/// The code globs an item answers for.
fn code_of<'a>(snapshot: &Snapshot, elements: &'a [Element], entry: &Entry) -> Vec<&'a GlobSet> {
    let id = entry.id.as_str();
    let own: Vec<&GlobSet> = elements.iter().filter(|e| e.id == id).map(|e| &e.globs).collect();
    if !own.is_empty() {
        return own;
    }
    let Some(item) = &entry.item else {
        return Vec::new();
    };
    elements
        .iter()
        .filter(|e| names(snapshot, &item.fields.components, &e.id))
        .map(|e| &e.globs)
        .collect()
}

/// Rows of the commit cache after one day: (day, repo, sha, path), for matching in memory.
/// Paths are relative to the product root, so project globs match whichever repo holds them.
struct Touches {
    rows: Vec<(String, String, String, String)>,
}

impl Touches {
    fn load(conn: &Connection, after: Date) -> Result<Touches, EngineError> {
        let mut stmt = conn.prepare(
            "SELECT substr(c.date, 1, 10), c.repo, c.sha, f.path FROM commits c \
             JOIN commit_files f ON f.repo = c.repo AND f.sha = c.sha WHERE substr(c.date, 1, 10) > ?1",
        )?;
        let rows = stmt
            .query_map(params![after.iso()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Touches { rows })
    }

    /// Commits after `since` with a file under one of `globs`.
    fn count(&self, since: Date, globs: &[&GlobSet]) -> u32 {
        let since = since.iso();
        let commits: HashSet<(&str, &str)> = self
            .rows
            .iter()
            .filter(|(day, _, _, path)| day.as_str() > since.as_str() && globs.iter().any(|g| g.is_match(path)))
            .map(|(_, repo, sha, _)| (repo.as_str(), sha.as_str()))
            .collect();
        commits.len() as u32
    }

    /// The commits after `since` that changed one of `files`.
    fn hits<'a>(
        &'a self,
        since: Date,
        files: &'a BTreeSet<String>,
    ) -> impl Iterator<Item = &'a (String, String, String, String)> {
        let since = since.iso();
        self.rows
            .iter()
            .filter(move |(day, _, _, path)| *day > since && files.contains(path))
    }

    /// Each of `files` changed after `since`, with its commit count.
    fn changed(&self, since: Date, files: &BTreeSet<String>) -> BTreeMap<String, u32> {
        let mut out: BTreeMap<String, u32> = BTreeMap::new();
        let mut seen: HashSet<(&str, &str, &str)> = HashSet::new();
        for (_, repo, sha, path) in self.hits(since, files) {
            if seen.insert((repo, sha, path)) {
                *out.entry(path.clone()).or_default() += 1;
            }
        }
        out
    }

    /// Distinct commits after `since` that changed one of `files`.
    fn commits_of(&self, since: Date, files: &BTreeSet<String>) -> u32 {
        let commits: HashSet<(&str, &str)> = self
            .hits(since, files)
            .map(|(_, repo, sha, _)| (repo.as_str(), sha.as_str()))
            .collect();
        commits.len() as u32
    }
}

/// A file under a doc root or a project file: plain markdown outside `stone/`.
fn is_plain(entry: &Entry) -> bool {
    !entry.path.starts_with("stone/")
}

/// The day of the last commit that touched `path`, a root-relative file.
fn last_commit_day(conn: &Connection, path: &str) -> Result<Option<Date>, EngineError> {
    let day: Option<String> = conn
        .query_row(
            "SELECT max(substr(c.date, 1, 10)) FROM commits c \
             JOIN commit_files f ON f.repo = c.repo AND f.sha = c.sha WHERE f.path = ?1",
            [path],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(day.as_deref().and_then(day_of))
}

/// The root-relative file a code span names, if it is one on disk: `path`, `path:12`,
/// `path:3-9` or `path#Symbol`, root-relative or relative to `folder`. Markdown is not code.
fn span_file(root: &Path, folder: Option<&str>, span: &str) -> Option<String> {
    let text = span.trim().replace('\\', "/");
    let text = text.split('#').next().unwrap_or_default();
    let text = match text.rsplit_once(':') {
        Some((path, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-') => path,
        _ => text,
    };
    let text = text.trim_start_matches("./");
    let bad = text.is_empty()
        || text.ends_with(".md")
        || text.ends_with('/')
        || text.starts_with('/')
        || text.contains([' ', '\t', ':', '*', '?', '<', '>', '|', '"', '`'])
        || text.split('/').any(|seg| seg == "..")
        || !text.contains('.');
    if bad {
        return None;
    }
    let mut tries = Vec::new();
    if let Some(folder) = folder.filter(|f| !f.is_empty()) {
        tries.push(format!("{folder}/{text}"));
    }
    tries.push(text.to_string());
    tries.into_iter().find(|path| root.join(path).is_file())
}

/// The code files a doc names in its body, root-relative: `[[code:...]]` targets (a symbol by its
/// file) and code spans outside fences that are a file path.
fn named_files(
    root: &Path,
    resolver: &mut CodeResolver,
    conn: &Connection,
    entry: &Entry,
) -> Result<BTreeSet<String>, EngineError> {
    let mut out = BTreeSet::new();
    let Some(item) = &entry.item else {
        return Ok(out);
    };
    for (_, target) in body_links(&item.body, item.body_line) {
        let Some(code) = target.strip_prefix("code:") else {
            continue;
        };
        match resolver.resolve(code)? {
            CodeLink::File(path) | CodeLink::Range { path, .. } => {
                out.insert(path);
            }
            CodeLink::Symbol(qname) => {
                let path: Option<String> = conn
                    .query_row(
                        "SELECT f.path FROM symbols s JOIN files f ON f.id = s.file_id \
                         WHERE s.qname = ?1 AND s.kind != 'file' ORDER BY f.path LIMIT 1",
                        [&qname],
                        |row| row.get(0),
                    )
                    .optional()?;
                out.extend(path);
            }
            _ => {}
        }
    }
    let folder = is_plain(entry).then(|| entry.path.rsplit_once('/').map_or("", |(folder, _)| folder));
    for (_, text, in_fence) in lines(&item.body, item.body_line) {
        if in_fence {
            continue;
        }
        for (a, b) in code_span_ranges(text) {
            let span = text[a..b].trim_matches('`');
            out.extend(span_file(root, folder, span));
        }
    }
    Ok(out)
}

/// One doc the named-code rule applies to.
struct Named {
    id: String,
    since: Date,
    label: &'static str,
    files: BTreeSet<String>,
    /// A plain doc takes no `verified` stamp from stone.
    plain: bool,
}

/// Every stale entry among `entries`, as (item id, why), in the order given.
fn find(
    repo: &Repo,
    snapshot: &Snapshot,
    entries: &[&Entry],
    today: Date,
) -> Result<Vec<(String, Stale)>, EngineError> {
    let elements = arch_elements(snapshot, &mut Vec::new());
    let threshold = repo.store.config.stale_commits;
    let mut found: Vec<(String, Stale)> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut named: Vec<Named> = Vec::new();
    let index = index_with_history(repo)?;
    let mut resolver = index
        .as_ref()
        .map(|index| CodeResolver::new(&repo.store.layout.root, index.conn()));

    for entry in entries {
        let Some(item) = &entry.item else { continue };
        if !is_plain(entry) && !can_be_stale(item.item_type()) {
            continue;
        }
        let id = entry.id.as_str().to_string();
        if let (Some(index), Some(resolver)) = (&index, resolver.as_mut()) {
            let files = named_files(&repo.store.layout.root, resolver, index.conn(), entry)?;
            let edited = last_commit_day(index.conn(), &entry.path)?.map(|day| (day, "edited"));
            let since = reference_day(entry)
                .into_iter()
                .chain(edited)
                .max_by_key(|(day, _)| *day);
            if let Some((since, label)) = since
                && !files.is_empty()
            {
                named.push(Named {
                    id: id.clone(),
                    since,
                    label,
                    files,
                    plain: is_plain(entry),
                });
            }
        }
        if let Some(due) = item.fields.stale_after.as_deref().and_then(day_of)
            && due < today
        {
            found.push((
                id,
                Stale {
                    reason: format!("`stale_after` {} has passed; re-read it and set a new date", due.iso()),
                    commits: 0,
                },
            ));
            continue;
        }
        let globs = code_of(snapshot, &elements, entry);
        if let Some((since, label)) = reference_day(entry)
            && !globs.is_empty()
        {
            candidates.push(Candidate {
                id,
                since,
                label,
                globs,
            });
        }
    }

    let earliest = candidates
        .iter()
        .map(|c| c.since)
        .chain(named.iter().map(|n| n.since))
        .min();
    // No git, or a checkout that cannot hold `.stone/`: the commit rules are skipped.
    if let (Some(earliest), Some(index)) = (earliest, &index) {
        let touches = Touches::load(index.conn(), earliest)?;
        for c in candidates {
            let commits = touches.count(c.since, &c.globs);
            if commits >= threshold {
                let reason = format!(
                    "{commits} commits changed its code since it was {} on {}; re-read it and update it or run `stone edit {} --verify --by <you>`",
                    c.label,
                    c.since.iso(),
                    c.id
                );
                found.push((c.id, Stale { reason, commits }));
            }
        }
        for n in named {
            if found.iter().any(|(id, _)| *id == n.id) {
                continue;
            }
            let changed = touches.changed(n.since, &n.files);
            if changed.is_empty() {
                continue;
            }
            let commits = touches.commits_of(n.since, &n.files);
            let mut files = changed.keys().map(|f| format!("`{f}`"));
            let first = files.next().unwrap_or_default();
            let names = match changed.len() {
                1 => first,
                n => format!("{first} and {} more files it names", n - 1),
            };
            let next = if n.plain {
                "re-read it and update it".to_string()
            } else {
                format!(
                    "re-read it and update it or run `stone edit {} --verify --by <you>`",
                    n.id
                )
            };
            let word = if commits == 1 { "commit" } else { "commits" };
            found.push((
                n.id,
                Stale {
                    reason: format!(
                        "{names} changed in {commits} {word} since it was {} on {}; {next}",
                        n.label,
                        n.since.iso()
                    ),
                    commits,
                },
            ));
        }
    }

    let order: HashMap<&str, usize> = entries.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    found.sort_by_key(|(id, _)| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
    Ok(found)
}

/// Why `input` is stale, or `None` when it is fresh, never goes stale (tasks) or its history is
/// unavailable. `today` is a parameter so tests do not depend on the clock.
pub fn staleness(repo: &Repo, input: &str, today: Date) -> Result<Option<Stale>, EngineError> {
    staleness_in(repo, &repo.snapshot()?, input, today)
}

/// `staleness` over a snapshot the caller already holds.
pub(crate) fn staleness_in(
    repo: &Repo,
    snapshot: &Snapshot,
    input: &str,
    today: Date,
) -> Result<Option<Stale>, EngineError> {
    let entry = snapshot.resolve(input)?.0;
    Ok(find(repo, snapshot, &[entry], today)?.pop().map(|(_, stale)| stale))
}

/// Why a plain doc (doc-root or project file) is stale, or `None`.
pub(crate) fn plain_staleness(repo: &Repo, entry: &Entry, today: Date) -> Result<Option<Stale>, EngineError> {
    Ok(find(repo, &repo.snapshot()?, &[entry], today)?
        .pop()
        .map(|(_, stale)| stale))
}

/// Every stale item as (item id, why), in the order of the store.
pub fn stale_items(repo: &Repo, today: Date) -> Result<Vec<(String, Stale)>, EngineError> {
    stale_in(repo, &repo.snapshot()?, today)
}

/// `stale_items` over a snapshot the caller already holds: store items only.
pub(crate) fn stale_in(repo: &Repo, snapshot: &Snapshot, today: Date) -> Result<Vec<(String, Stale)>, EngineError> {
    let entries: Vec<&Entry> = snapshot.entries.iter().collect();
    find(repo, snapshot, &entries, today)
}

/// Store items and plain docs that are stale, as (path, why), for `check`.
pub(crate) fn stale_paths(
    repo: &Repo,
    snapshot: &Snapshot,
    docs: &[Entry],
    today: Date,
) -> Result<Vec<(String, Stale)>, EngineError> {
    let entries: Vec<&Entry> = snapshot.entries.iter().chain(docs).collect();
    let found = find(repo, snapshot, &entries, today)?;
    Ok(found
        .into_iter()
        .filter_map(|(id, stale)| {
            let entry = entries.iter().find(|e| e.id.as_str() == id)?;
            Some((entry.path.clone(), stale))
        })
        .collect())
}
