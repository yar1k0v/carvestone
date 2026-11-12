//! `stone precedents`: finished tasks that made the same kind of change as a task, a component or
//! a role, so an agent can copy the pattern of a similar finished task (ADR-12).
//!
//! A task's files are the files of the commits whose `Refs` trailer names it. Each file has a
//! component (the arch element that owns it), a role (test, entry point, else
//! the main symbol kind of the file) and a status; pairs such as "added entry point" or "modified
//! test" are the task's shape. Finished tasks rank by shared components, shared shape pairs and a
//! shared folder; words only break ties, and rank alone when the lookup has nothing else.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};

use carvestone_core::ItemType;
use carvestone_graph::is_test_path;
use carvestone_store::Entry;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::code_entries::detect;
use crate::git_cache::index_with_history;
use crate::owner::{Element, arch_elements};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::tiers;

/// Precedents listed when no limit is given.
pub const DEFAULT_LIMIT: usize = 8;
/// Changed files shown per precedent.
const FILES_SHOWN: usize = 4;
/// Matched signals shown per precedent line; the JSON carries them all.
const SIGNALS_SHOWN: usize = 4;
/// Commits naming more tasks than this say nothing about one task's shape.
const MAX_TASKS_PER_COMMIT: u32 = 3;
/// A task that changed more files than this (a release, a bulk import) says nothing about the
/// shape of a normal change.
const MAX_PRECEDENT_FILES: usize = 100;
const COMPONENT_POINTS: u32 = 3;
const SHAPE_POINTS: u32 = 2;
const FOLDER_POINTS: u32 = 1;
/// Words too common in task titles to say anything about the change.
const STOP_WORDS: [&str; 10] = [
    "this", "that", "with", "from", "into", "when", "then", "than", "task", "tasks",
];

/// What to find precedents for: a task, or a component and/or a role named directly.
#[derive(Debug, Clone, Default)]
pub struct Lookup {
    pub task: Option<String>,
    pub component: Option<String>,
    pub role: Option<String>,
    /// Most precedents to list; `DEFAULT_LIMIT` when absent.
    pub limit: Option<usize>,
    /// Cut the answer to about this many tokens.
    pub budget: Option<usize>,
}

/// A finished task that made a similar change, and why it matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Precedent {
    pub id: String,
    pub title: Option<String>,
    pub score: u32,
    /// The signals that matched: `component arch/x`, `added test`, `folder src/a`, `words ...`.
    pub matched: Vec<String>,
    /// Up to four changed files, then `more_files` counts the rest.
    pub files: Vec<String>,
    pub more_files: usize,
}

impl Precedent {
    pub(crate) fn line(&self) -> String {
        let mut line = self.id.clone();
        if let Some(title) = &self.title {
            line.push_str("  ");
            line.push_str(title);
        }
        line.push_str("  matched ");
        line.push_str(&self.matched[..self.matched.len().min(SIGNALS_SHOWN)].join(", "));
        if self.matched.len() > SIGNALS_SHOWN {
            line.push_str(&format!(" +{} more", self.matched.len() - SIGNALS_SHOWN));
        }
        if !self.files.is_empty() {
            line.push_str("  files ");
            line.push_str(&self.files.join(", "));
            if self.more_files > 0 {
                line.push_str(&format!(" +{} more", self.more_files));
            }
        }
        line
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Precedents {
    /// What the lookup was based on, and a note when it had to fall back to words.
    pub basis: String,
    /// `words` when no component, shape or folder was known and titles were compared instead.
    pub fallback: Option<String>,
    pub hits: Vec<Precedent>,
    /// The limit or the budget cut lines off.
    pub truncated: bool,
    /// Precedents the limit or the budget left out.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    pub omitted: usize,
    /// The size of the precedents the limit or the budget left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_out: Option<tiers::LeftOut>,
}

impl Precedents {
    /// The printed lines: the basis, then one per precedent.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!("based on: {}", self.basis)];
        if self.hits.is_empty() && self.omitted == 0 {
            out.push("no finished task made a similar change".to_string());
        }
        out.extend(self.hits.iter().map(Precedent::line));
        if let Some(left) = &self.left_out {
            out.push(tiers::left_out_line(left, false, "--limit or --budget"));
        }
        out
    }

    /// Tokens the printed lines cost an agent (characters divided by four).
    pub fn cost(&self) -> usize {
        self.lines().iter().map(|l| tiers::tokens(l)).sum()
    }

    /// Drops the weakest precedents until the cost fits `budget`. `dropped` holds the printed
    /// lines the limit already cut.
    fn fit(&mut self, budget: usize, mut dropped: Vec<String>) {
        while self.cost() > budget {
            let Some(last) = self.hits.pop() else {
                break;
            };
            dropped.push(last.line());
            self.truncated = true;
            self.omitted += 1;
            self.left_out = tiers::left_out("precedents", &dropped);
        }
    }
}

/// The finished tasks most like the lookup, best first. Finished tasks that changed more than
/// `MAX_PRECEDENT_FILES` files are left out.
pub fn precedents(repo: &Repo, lookup: &Lookup) -> Result<Precedents, EngineError> {
    if lookup.task.is_none() && lookup.component.is_none() && lookup.role.is_none() {
        return Err(EngineError::Invalid(
            "give a task, --component or --role, for example `stone precedents CS-12`".to_string(),
        ));
    }
    let snapshot = repo.snapshot()?;
    let elements = arch_elements(&snapshot, &mut Vec::new());
    let index = index_with_history(repo)?;
    let conn = index.as_ref().map(|i| i.conn());

    let target = match &lookup.task {
        Some(input) => {
            let (entry, _) = snapshot.resolve(input)?;
            if !is_task(entry) {
                return Err(EngineError::Invalid(format!(
                    "{} is not a task; give a task id, for example CS-12",
                    snapshot.label(entry)
                )));
            }
            Some(entry)
        }
        None => None,
    };
    let target_files = match (target, conn) {
        (Some(entry), Some(conn)) => commit_files(conn, entry)?,
        _ => Vec::new(),
    };
    let mut finished: Vec<(&Entry, Vec<(char, String)>)> = Vec::new();
    for entry in &snapshot.entries {
        if !is_task(entry) || status(entry) != Some("done") || target.is_some_and(|t| t.id == entry.id) {
            continue;
        }
        let files = match conn {
            Some(conn) => commit_files(conn, entry)?,
            None => Vec::new(),
        };
        if files.len() > MAX_PRECEDENT_FILES {
            continue;
        }
        finished.push((entry, files));
    }

    let paths: HashSet<String> = target_files
        .iter()
        .chain(finished.iter().flat_map(|(_, files)| files))
        .map(|(_, path)| path.clone())
        .collect();
    let mut roles = Roles::new(repo, conn, &paths)?;

    let (profile, mut basis) = match target {
        Some(entry) => {
            let label = snapshot.label(entry);
            if target_files.is_empty() {
                let (planned, from_fields) = planned_profile(&snapshot, &elements, entry);
                let basis = if from_fields {
                    format!("the components and paths of {label} (no commit names it yet)")
                } else if planned.is_empty() {
                    format!("the words in the title of {label} (it has no commits, components or paths)")
                } else {
                    format!("the wording of {label} (it has no commits, components or paths)")
                };
                (planned, basis)
            } else {
                (
                    profile_of(&target_files, &elements, &mut roles)?,
                    format!("the commits of {label}"),
                )
            }
        }
        None => direct_profile(&snapshot, lookup)?,
    };
    let by_words = profile.is_empty() || conn.is_none();
    if conn.is_none() {
        basis.push_str("; git history is unavailable, so finished tasks have no files");
    }
    let words = target.map(title_words).unwrap_or_default();

    let mut hits = rank(&snapshot, &elements, &mut roles, &profile, by_words, &words, finished)?;
    let limit = lookup.limit.unwrap_or(DEFAULT_LIMIT);
    let omitted = hits.len().saturating_sub(limit);
    let dropped: Vec<String> = hits
        .split_off(hits.len().min(limit))
        .iter()
        .map(Precedent::line)
        .collect();
    let mut result = Precedents {
        basis,
        fallback: by_words.then(|| "words".to_string()),
        hits,
        truncated: omitted > 0,
        omitted,
        left_out: tiers::left_out("precedents", &dropped),
    };
    if let Some(budget) = lookup.budget {
        result.fit(budget, dropped);
    }
    Ok(result)
}

/// The finished tasks that share points (or title words, with `by_words`) with the profile, best
/// first.
fn rank(
    snapshot: &Snapshot,
    elements: &[Element],
    roles: &mut Roles<'_>,
    profile: &Profile,
    by_words: bool,
    words: &BTreeSet<String>,
    finished: Vec<(&Entry, Vec<(char, String)>)>,
) -> Result<Vec<Precedent>, EngineError> {
    let mut ranked: Vec<(u32, usize, u32, Precedent)> = Vec::new();
    for (entry, files) in finished {
        let theirs = profile_of(&files, elements, roles)?;
        let shared_words: Vec<String> = words.intersection(&title_words(entry)).cloned().collect();
        let (points, mut matched) = if by_words {
            (0, Vec::new())
        } else {
            score(profile, &theirs)
        };
        let keep = if by_words { !shared_words.is_empty() } else { points > 0 };
        if !keep {
            continue;
        }
        if by_words {
            matched.push(format!(
                "words {}",
                shared_words.iter().take(3).cloned().collect::<Vec<_>>().join(" ")
            ));
        }
        let mut shown: Vec<String> = files.into_iter().map(|(_, path)| path).collect();
        shown.sort();
        shown.dedup();
        let more_files = shown.len().saturating_sub(FILES_SHOWN);
        shown.truncate(FILES_SHOWN);
        let number = snapshot.scheme.number(&entry.id).map_or(0, |(_, n)| n);
        ranked.push((
            points,
            shared_words.len(),
            number,
            Precedent {
                id: snapshot.label(entry),
                title: entry.item.as_ref().and_then(|i| i.fields.title.clone()),
                score: points,
                matched,
                files: shown,
                more_files,
            },
        ));
    }
    ranked.sort_by(|a, b| {
        (Reverse(a.0), Reverse(a.1), Reverse(a.2), &a.3.id).cmp(&(Reverse(b.0), Reverse(b.1), Reverse(b.2), &b.3.id))
    });
    Ok(ranked.into_iter().map(|(_, _, _, p)| p).collect())
}

pub(crate) fn is_task(entry: &Entry) -> bool {
    entry.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Task)
}

fn status(entry: &Entry) -> Option<&str> {
    entry.item.as_ref().and_then(|i| i.fields.status.as_deref())
}

/// The newest status and the path of every file the commits naming the task changed, outside
/// `stone/`. A commit that names more tasks than `MAX_TASKS_PER_COMMIT` says nothing about the
/// shape of one of them, so it is left out.
fn commit_files(conn: &Connection, entry: &Entry) -> Result<Vec<(char, String)>, EngineError> {
    let mut stmt = conn.prepare_cached(
        "SELECT f.path, f.status FROM commit_refs r \
         JOIN commits c ON c.repo = r.repo AND c.sha = r.sha \
         JOIN commit_files f ON f.repo = c.repo AND f.sha = c.sha \
         WHERE r.kind = 'task' AND r.ref_id = ?1 \
         AND (SELECT COUNT(DISTINCT x.ref_id) FROM commit_refs x \
              WHERE x.repo = c.repo AND x.sha = c.sha AND x.kind = 'task') <= ?2 \
         ORDER BY c.ts DESC, c.sha ASC, c.repo ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![entry.id.as_str(), MAX_TASKS_PER_COMMIT], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for row in rows {
        let (path, status) = row?;
        if path.starts_with("stone/") || path.starts_with(".stone/") || !seen.insert(path.clone()) {
            continue;
        }
        files.push((status.chars().next().unwrap_or('M'), path));
    }
    Ok(files)
}

/// The components, shape pairs and folders of a set of changed files.
#[derive(Default)]
struct Profile {
    components: BTreeSet<String>,
    shapes: BTreeSet<String>,
    folders: BTreeSet<String>,
}

impl Profile {
    fn is_empty(&self) -> bool {
        self.components.is_empty() && self.shapes.is_empty() && self.folders.is_empty()
    }
}

fn shape_of(status: char, role: &str) -> String {
    let verb = match status {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        _ => "modified",
    };
    format!("{verb} {role}")
}

fn profile_of(files: &[(char, String)], elements: &[Element], roles: &mut Roles) -> Result<Profile, EngineError> {
    let mut profile = Profile::default();
    for (status, path) in files {
        if let Some(owner) = elements.iter().find(|e| e.owns(path)) {
            profile.components.insert(owner.label.clone());
        }
        profile.shapes.insert(shape_of(*status, &roles.role(path)?));
        if let Some((dir, _)) = path.rsplit_once('/') {
            profile.folders.insert(dir.to_string());
        }
    }
    Ok(profile)
}

/// The profile of a lookup that names a component and/or a role instead of a task.
fn direct_profile(snapshot: &Snapshot, lookup: &Lookup) -> Result<(Profile, String), EngineError> {
    let mut profile = Profile::default();
    let mut parts = Vec::new();
    if let Some(input) = &lookup.component {
        let (entry, _) = snapshot.resolve(input)?;
        let label = snapshot.label(entry);
        profile.components.insert(label.clone());
        parts.push(format!("component {label}"));
    }
    if let Some(role) = &lookup.role {
        for verb in ["added", "modified"] {
            profile.shapes.insert(format!("{verb} {role}"));
        }
        parts.push(format!("role {role}"));
    }
    Ok((profile, parts.join(" and ")))
}

/// The profile of a task no commit names yet: its `components` and `paths` fields; else the roles
/// and component names its wording points at. The flag says the fields gave it.
fn planned_profile(snapshot: &Snapshot, elements: &[Element], entry: &Entry) -> (Profile, bool) {
    let mut profile = Profile::default();
    let Some(item) = &entry.item else {
        return (profile, false);
    };
    for raw in &item.fields.components {
        if let Ok((component, _)) = snapshot.resolve(raw) {
            profile.components.insert(snapshot.label(component));
        }
    }
    for raw in &item.fields.paths {
        let base = raw.trim().trim_start_matches("./").trim_end_matches(['*', '/']);
        if base.is_empty() {
            continue;
        }
        let names_file = base.rsplit('/').next().is_some_and(|last| last.contains('.'));
        let folder = match base.rsplit_once('/') {
            Some((dir, _)) if names_file => dir,
            _ if names_file => "",
            _ => base,
        };
        if !folder.is_empty() {
            profile.folders.insert(folder.to_string());
        }
        for owner in elements.iter().filter(|e| e.owns(base) || e.owns(&format!("{base}/x"))) {
            profile.components.insert(owner.label.clone());
        }
    }
    if !profile.is_empty() {
        return (profile, true);
    }

    let body = entry.text.splitn(3, "---").nth(2).unwrap_or(&entry.text);
    let text = format!("{} {body}", item.fields.title.as_deref().unwrap_or(""));
    let tokens: HashSet<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |words: &[&str]| words.iter().any(|w| tokens.contains(*w));
    let verb = if has(&["add", "adds", "added", "new", "create", "creates", "introduce"]) {
        'A'
    } else {
        'M'
    };
    if has(&["test", "tests"]) {
        profile.shapes.insert(shape_of(verb, "test"));
    }
    for owner in elements {
        let name = owner.label.rsplit('/').next().unwrap_or(&owner.label).to_lowercase();
        if name.chars().count() >= 3 && tokens.contains(&name) {
            profile.components.insert(owner.label.clone());
        }
    }
    (profile, false)
}

/// Points and the signals behind them: shared components, shared shape pairs, one shared folder.
fn score(target: &Profile, other: &Profile) -> (u32, Vec<String>) {
    let mut points = 0;
    let mut matched = Vec::new();
    for component in target.components.intersection(&other.components) {
        points += COMPONENT_POINTS;
        matched.push(format!("component {component}"));
    }
    for shape in target.shapes.intersection(&other.shapes) {
        points += SHAPE_POINTS;
        matched.push(shape.clone());
    }
    let folder = target
        .folders
        .iter()
        .find(|a| other.folders.iter().any(|b| nested(a, b)));
    if let Some(folder) = folder {
        points += FOLDER_POINTS;
        matched.push(format!("folder {folder}"));
    }
    (points, matched)
}

/// Whether one folder is the other or lies inside it.
fn nested(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn title_words(entry: &Entry) -> BTreeSet<String> {
    let title = entry
        .item
        .as_ref()
        .and_then(|i| i.fields.title.as_deref())
        .unwrap_or("");
    title
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 4 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// The role of a changed file, read from today's code and cached per path.
struct Roles<'a> {
    conn: Option<&'a Connection>,
    /// The files that hold an entry point.
    entries: HashSet<String>,
    cache: HashMap<String, String>,
}

impl<'a> Roles<'a> {
    fn new(repo: &Repo, conn: Option<&'a Connection>, paths: &HashSet<String>) -> Result<Self, EngineError> {
        let mut entries = HashSet::new();
        if let Some(conn) = conn {
            for found in detect(conn, &repo.store.layout.root, &|path| paths.contains(path))? {
                entries.insert(found.path);
            }
        }
        Ok(Roles {
            conn,
            entries,
            cache: HashMap::new(),
        })
    }

    fn role(&mut self, path: &str) -> Result<String, EngineError> {
        if let Some(role) = self.cache.get(path) {
            return Ok(role.clone());
        }
        let role = self.classify(path)?;
        self.cache.insert(path.to_string(), role.clone());
        Ok(role)
    }

    fn classify(&self, path: &str) -> Result<String, EngineError> {
        if is_test_path(path) {
            return Ok("test".to_string());
        }
        if self.entries.contains(path) {
            return Ok("entry point".to_string());
        }
        let Some(conn) = self.conn else {
            return Ok("file".to_string());
        };
        let kind: Option<String> = conn
            .query_row(
                "SELECT s.kind FROM symbols s JOIN files f ON f.id = s.file_id \
                 LEFT JOIN symbols p ON p.id = s.parent_id \
                 WHERE f.path = ?1 AND s.kind NOT IN ('file', 'project') \
                 AND (s.parent_id IS NULL OR p.kind IN ('file', 'project')) \
                 GROUP BY s.kind ORDER BY COUNT(*) DESC, MIN(s.line_start) LIMIT 1",
                [path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(kind.unwrap_or_else(|| "file".to_string()))
    }
}
