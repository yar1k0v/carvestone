//! `stone why`: from a file or one line to the commits, tasks, plans and
//! decisions behind it. Trailers answer when commits carry them; the owning
//! component and the task `paths` answer when they do not, and the result
//! says which was used.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use carvestone_core::ItemType;
use carvestone_store::Entry;
use globset::GlobBuilder;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::check_links::{CodeLink, CodeResolver};
use crate::git_cache::{cached_blame, ensure_filled};
use crate::owner::arch_elements;
use crate::refresh::refresh;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::tiers;

/// Commits listed when no line is given.
const RECENT: usize = 5;

/// A commit that explains the file or line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CommitRef {
    /// Empty for a change that is not committed yet.
    pub sha: String,
    pub date: String,
    pub author: String,
    pub subject: String,
    pub uncommitted: bool,
}

/// A task, plan, decision or doc that explains the file, and how it was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Related {
    pub id: String,
    pub status: Option<String>,
    pub title: Option<String>,
    /// `trailer`, `task`, `component`, `path`, `code link`.
    pub via: String,
    /// Commits that name it; zero when it was not found through a trailer.
    pub commits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Why {
    pub path: String,
    pub line: Option<u32>,
    pub component: Option<String>,
    pub commits: Vec<CommitRef>,
    pub tasks: Vec<Related>,
    pub plans: Vec<Related>,
    pub decisions: Vec<Related>,
    pub docs: Vec<Related>,
    /// `component` or `path` when the commits named nothing; absent otherwise.
    pub fallback: Option<String>,
    /// The clone is shallow, so older commits are missing.
    pub shallow: bool,
    /// The budget cut lines off.
    pub truncated: bool,
    /// Entries the budget left out.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of the entries the budget left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
}

impl Why {
    /// The printed lines, one per entry, a heading before each non-empty block.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let at = match self.line {
            Some(n) => format!("{}:{n}", self.path),
            None => self.path.clone(),
        };
        out.push(format!("{at}  {}", self.component.as_deref().unwrap_or("no component")));
        if self.shallow {
            out.push("history is cut: this is a shallow clone".to_string());
        }
        // A fallback that found nothing says so instead of promising tasks it does not show.
        let none = self.tasks.is_empty() && self.omitted == 0;
        match (self.fallback.as_deref(), none) {
            (Some("component"), false) => out
                .push("no commit names a task, plan or decision; showing the component's open tasks".to_string()),
            (Some("component"), true) => out.push(
                "no commit names a task, plan or decision, and the component has no open tasks".to_string(),
            ),
            (Some(_), false) => out.push(
                "no commit names a task, plan or decision and no component owns the file; showing tasks that list it in paths"
                    .to_string(),
            ),
            (Some(_), true) => out.push(
                "no commit names a task, plan or decision, no component owns the file and no task lists it in paths"
                    .to_string(),
            ),
            (None, _) => {}
        }
        if self.commits.is_empty() && self.omitted == 0 {
            out.push("no commits touch it yet".to_string());
        }
        block(&mut out, "commits", self.commits.iter().map(commit_line));
        block(&mut out, "tasks", self.tasks.iter().map(related_line));
        block(&mut out, "plans", self.plans.iter().map(related_line));
        block(&mut out, "decisions", self.decisions.iter().map(related_line));
        block(&mut out, "docs", self.docs.iter().map(related_line));
        if let Some(left) = &self.left_out {
            out.push(tiers::left_out_line(left, false, "--budget"));
        }
        out
    }

    /// Tokens the printed lines cost an agent (characters divided by four).
    pub fn cost(&self) -> usize {
        self.lines().iter().map(|l| l.chars().count().div_ceil(4)).sum()
    }

    /// Drops the least useful entries until the cost fits `budget`.
    fn fit(&mut self, budget: usize) {
        // The printed lines of the dropped entries, the last one dropped first.
        let mut dropped: Vec<String> = Vec::new();
        while self.cost() > budget {
            let line = self
                .docs
                .pop()
                .map(|r| related_line(&r))
                .or_else(|| self.plans.pop().map(|r| related_line(&r)))
                .or_else(|| self.decisions.pop().map(|r| related_line(&r)))
                .or_else(|| self.commits.pop().map(|c| commit_line(&c)))
                .or_else(|| self.tasks.pop().map(|r| related_line(&r)));
            let Some(line) = line else {
                break;
            };
            dropped.insert(0, format!("  {line}"));
            self.truncated = true;
            self.omitted += 1;
            self.left_out = tiers::left_out("entries", &dropped);
        }
    }
}

fn block(out: &mut Vec<String>, name: &str, lines: impl Iterator<Item = String>) {
    let lines: Vec<String> = lines.collect();
    if lines.is_empty() {
        return;
    }
    out.push(format!("{name}:"));
    out.extend(lines.into_iter().map(|l| format!("  {l}")));
}

fn commit_line(c: &CommitRef) -> String {
    if c.uncommitted {
        return "uncommitted change".to_string();
    }
    let day = c.date.get(..10).unwrap_or(&c.date);
    format!(
        "{}  {day}  {}  {}",
        c.sha.get(..7).unwrap_or(&c.sha),
        c.author,
        c.subject
    )
}

fn related_line(r: &Related) -> String {
    let mut line = r.id.clone();
    for part in [r.status.as_deref(), r.title.as_deref()].into_iter().flatten() {
        line.push_str("  ");
        line.push_str(part);
    }
    line.push_str("  via ");
    line.push_str(&r.via);
    match r.commits {
        0 => {}
        1 => line.push_str("  1 commit"),
        n => line.push_str(&format!("  {n} commits")),
    }
    line
}

/// Splits `path:line` input; a suffix that is not a number stays in the path.
pub fn parse_target(input: &str) -> (String, Option<u32>) {
    if let Some((path, tail)) = input.rsplit_once(':')
        && !path.is_empty()
        && let Ok(line) = tail.parse::<u32>()
    {
        return (path.to_string(), Some(line));
    }
    (input.to_string(), None)
}

/// `path` relative to the repository folder, with forward slashes.
fn relative_path(root: &Path, input: &str) -> Result<String, EngineError> {
    let all = input.trim().replace('\\', "/");
    let root = root.to_string_lossy().replace('\\', "/");
    let root = root.trim_end_matches('/');
    let rel = match all.strip_prefix(root) {
        Some(rest) if rest.starts_with('/') => rest.trim_start_matches('/'),
        _ => all.as_str(),
    };
    let rel = rel.strip_prefix("./").unwrap_or(rel);
    if rel.is_empty() {
        return Err(EngineError::Invalid(
            "give a file path, for example src/a.cs or src/a.cs:5".to_string(),
        ));
    }
    Ok(rel.to_string())
}

fn count_lines(bytes: &[u8]) -> u32 {
    let breaks = bytes.iter().filter(|b| **b == b'\n').count();
    let open = usize::from(bytes.last().is_some_and(|b| *b != b'\n'));
    (breaks + open) as u32
}

pub fn why(repo: &Repo, path: &str, line: Option<u32>, budget: Option<usize>) -> Result<Why, EngineError> {
    let root = repo.store.layout.root.clone();
    let path = relative_path(&root, path)?;
    // Git speaks in the paths of the repo that holds the file; everything stored stays root-relative.
    let repos = repo.repos();
    let Some((one, inner)) = repos.repo_of(&path) else {
        // A missing or too old git is a problem of the machine, not of the folder.
        carvestone_git::check_installed()?;
        return Err(EngineError::Invalid(format!(
            "{path} is not under git; run `git init` in its folder or list the folder in code_roots"
        )));
    };
    let git = one.git();
    if !git.is_tracked(&inner)? {
        return Err(EngineError::Invalid(format!(
            "{path} is not tracked by git; run `git add`"
        )));
    }
    if let Some(n) = line {
        let bytes = std::fs::read(root.join(&path))
            .map_err(|_| EngineError::Invalid(format!("{path} cannot be read; restore it or check the path")))?;
        let total = count_lines(&bytes);
        if n == 0 {
            return Err(EngineError::Invalid("lines start at 1".to_string()));
        }
        if n > total {
            let s = if total == 1 { "" } else { "s" };
            return Err(EngineError::Invalid(format!("{path} has {total} line{s}")));
        }
    }

    refresh(repo)?;
    ensure_filled(repo)?;
    let shallow = git.is_shallow().unwrap_or(false);
    let in_head = git.head_blob(&inner)?.is_some();

    // Which commits explain the file: the line's own, else the newest touching the file.
    let mut shas: Vec<String> = Vec::new();
    let mut uncommitted = false;
    match line {
        Some(n) if in_head => {
            let blamed = cached_blame(repo, &path, (n, n))?;
            match blamed.first() {
                Some(b) if !b.is_uncommitted() => shas.push(b.sha.clone()),
                _ => uncommitted = true,
            }
        }
        Some(_) => uncommitted = true,
        None => {}
    }
    if (line.is_none() || uncommitted) && in_head {
        shas.extend(git.file_log(&inner, true, RECENT)?.into_iter().map(|c| c.sha));
    }

    let snapshot = repo.snapshot()?;
    let index = repo.index()?;
    let conn = index.conn();

    let mut commits = Vec::new();
    if uncommitted {
        commits.push(CommitRef {
            sha: String::new(),
            date: String::new(),
            author: String::new(),
            subject: "uncommitted change".to_string(),
            uncommitted: true,
        });
    }
    // ref id -> (trailer kind, commits that name it)
    let mut named: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for sha in &shas {
        let row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT date, author, subject FROM commits WHERE repo = ?1 AND sha = ?2",
                params![one.dir(), sha],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((date, author, subject)) = row else {
            continue;
        };
        commits.push(CommitRef {
            sha: sha.clone(),
            date,
            author,
            subject,
            uncommitted: false,
        });
        let mut stmt =
            conn.prepare_cached("SELECT ref_id, kind FROM commit_refs WHERE repo = ?1 AND sha = ?2 ORDER BY rowid")?;
        let refs = stmt.query_map(params![one.dir(), sha], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for pair in refs {
            let (ref_id, kind) = pair?;
            named
                .entry(ref_id)
                .or_insert_with(|| (kind, BTreeSet::new()))
                .1
                .insert(sha.clone());
        }
    }

    let mut lists = Lists::default();
    let mut task_entries: Vec<&Entry> = Vec::new();
    for (ref_id, (kind, by)) in named {
        let count = by.len() as u32;
        match find(&snapshot, &ref_id) {
            Some(entry) => {
                let rel = related(&snapshot, entry, "trailer", count);
                match item_type(entry) {
                    Some(ItemType::Task) => {
                        task_entries.push(entry);
                        lists.tasks.push(rel);
                    }
                    Some(ItemType::Plan) => lists.plans.push(rel),
                    Some(ItemType::Decision) => lists.decisions.push(rel),
                    _ => lists.docs.push(rel),
                }
            }
            None => {
                let rel = Related {
                    id: ref_id,
                    status: None,
                    title: None,
                    via: "trailer".to_string(),
                    commits: count,
                };
                match kind.as_str() {
                    "plan" => lists.plans.push(rel),
                    "decision" => lists.decisions.push(rel),
                    _ => lists.tasks.push(rel),
                }
            }
        }
    }
    for list in [
        &mut lists.tasks,
        &mut lists.plans,
        &mut lists.decisions,
        &mut lists.docs,
    ] {
        list.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.id.cmp(&b.id)));
    }

    let mut problems = Vec::new();
    let owner = arch_elements(&snapshot, &mut problems)
        .into_iter()
        .find(|e| e.owns(&path));

    let mut fallback = None;
    if lists.tasks.is_empty() && lists.plans.is_empty() && lists.decisions.is_empty() {
        let via = if owner.is_some() { "component" } else { "path" };
        fallback = Some(via.to_string());
        for entry in open_tasks(&snapshot, &path, owner.as_ref().map(|o| o.id.as_str())) {
            lists.tasks.push(related(&snapshot, entry, via, 0));
            task_entries.push(entry);
        }
    }
    for entry in task_entries {
        enrich(&snapshot, entry, &mut lists);
    }

    if let Some(o) = &owner {
        if let Some(entry) = find(&snapshot, &o.id) {
            for link in links_of(entry) {
                if let Some(target) = target_of(&snapshot, link, ItemType::Decision) {
                    lists.add_decision(related(&snapshot, target, "component", 0));
                }
            }
        }
        for entry in &snapshot.entries {
            if is_doc_like(entry) && names(&snapshot, &components_of(entry), &o.id) {
                lists.add_doc(related(&snapshot, entry, "component", 0));
            }
        }
    }
    for id in linked_docs(&root, conn, &path)? {
        if let Some(entry) = find(&snapshot, &id)
            && is_doc_like(entry)
            && owner.as_ref().is_none_or(|o| o.id != id)
        {
            lists.add_doc(related(&snapshot, entry, "code link", 0));
        }
    }

    let mut result = Why {
        path,
        line,
        component: owner.map(|o| o.label),
        commits,
        tasks: lists.tasks,
        plans: lists.plans,
        decisions: lists.decisions,
        docs: lists.docs,
        fallback,
        shallow,
        truncated: false,
        omitted: 0,
        left_out: None,
    };
    if let Some(budget) = budget {
        result.fit(budget);
    }
    Ok(result)
}

#[derive(Default)]
struct Lists {
    tasks: Vec<Related>,
    plans: Vec<Related>,
    decisions: Vec<Related>,
    docs: Vec<Related>,
}

impl Lists {
    fn add_plan(&mut self, rel: Related) {
        push_new(&mut self.plans, rel);
    }
    fn add_decision(&mut self, rel: Related) {
        push_new(&mut self.decisions, rel);
    }
    fn add_doc(&mut self, rel: Related) {
        push_new(&mut self.docs, rel);
    }
}

fn push_new(list: &mut Vec<Related>, rel: Related) {
    if !list.iter().any(|r| r.id == rel.id) {
        list.push(rel);
    }
}

/// The plan and the decisions a task points at.
fn enrich(snapshot: &Snapshot, task: &Entry, lists: &mut Lists) {
    let Some(item) = &task.item else {
        return;
    };
    if let Some(plan) = item.fields.plan.as_deref()
        && let Some(target) = target_of(snapshot, plan, ItemType::Plan)
    {
        lists.add_plan(related(snapshot, target, "task", 0));
    }
    for link in &item.fields.links {
        if let Some(target) = target_of(snapshot, link, ItemType::Decision) {
            lists.add_decision(related(snapshot, target, "task", 0));
        } else if let Some(target) = target_of(snapshot, link, ItemType::Plan) {
            lists.add_plan(related(snapshot, target, "task", 0));
        }
    }
}

fn item_type(entry: &Entry) -> Option<ItemType> {
    entry.item.as_ref().and_then(|i| i.item_type())
}

fn is_doc_like(entry: &Entry) -> bool {
    matches!(
        item_type(entry),
        Some(ItemType::Doc | ItemType::Reference | ItemType::System | ItemType::Container | ItemType::Component)
    )
}

fn links_of(entry: &Entry) -> &[String] {
    entry.item.as_ref().map_or(&[], |i| i.fields.links.as_slice())
}

fn components_of(entry: &Entry) -> Vec<String> {
    entry
        .item
        .as_ref()
        .map_or_else(Vec::new, |i| i.fields.components.clone())
}

fn find<'a>(snapshot: &'a Snapshot, id: &str) -> Option<&'a Entry> {
    snapshot.entries.iter().find(|e| e.id.as_str() == id)
}

/// The entry `raw` names when it is of type `wanted`.
fn target_of<'a>(snapshot: &'a Snapshot, raw: &str, wanted: ItemType) -> Option<&'a Entry> {
    let (entry, _) = snapshot.resolve(raw).ok()?;
    (item_type(entry) == Some(wanted)).then_some(entry)
}

/// Whether any of `raws` names the item `id`.
pub(crate) fn names(snapshot: &Snapshot, raws: &[String], id: &str) -> bool {
    raws.iter()
        .any(|raw| snapshot.resolve(raw).is_ok_and(|(e, _)| e.id.as_str() == id))
}

fn related(snapshot: &Snapshot, entry: &Entry, via: &str, commits: u32) -> Related {
    let fields = entry.item.as_ref().map(|i| &i.fields);
    Related {
        id: snapshot.label(entry),
        status: fields.and_then(|f| f.status.clone()),
        title: fields.and_then(|f| f.title.clone()),
        via: via.to_string(),
        commits,
    }
}

/// Tasks not done whose `components` name the owner or whose `paths` cover the file.
fn open_tasks<'a>(snapshot: &'a Snapshot, path: &str, owner: Option<&str>) -> Vec<&'a Entry> {
    let mut found: Vec<&Entry> = snapshot
        .entries
        .iter()
        .filter(|entry| {
            let Some(item) = &entry.item else {
                return false;
            };
            if item.item_type() != Some(ItemType::Task) {
                return false;
            }
            if matches!(item.fields.status.as_deref(), Some("done" | "archived")) {
                return false;
            }
            owner.is_some_and(|id| names(snapshot, &item.fields.components, id)) || covers(&item.fields.paths, path)
        })
        .collect();
    found.sort_by_key(|e| {
        (
            snapshot.scheme.number(&e.id).map_or(u32::MAX, |(_, n)| n),
            e.path.clone(),
        )
    });
    found
}

/// Whether one of the `patterns` (globs, files or folders) matches `path`.
pub(crate) fn covers(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|raw| {
        let pattern = raw.trim().trim_start_matches("./");
        if pattern.is_empty() {
            return false;
        }
        if pattern == path || path.starts_with(&format!("{}/", pattern.trim_end_matches('/'))) {
            return true;
        }
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .is_ok_and(|g| g.compile_matcher().is_match(path))
    })
}

/// Ids of the items with a `[[code:...]]` link that points at the file or a symbol in it.
fn linked_docs(root: &Path, conn: &Connection, path: &str) -> Result<Vec<String>, EngineError> {
    let mut written_stmt = conn.prepare("SELECT DISTINCT dst_item FROM links WHERE kind = 'code' ORDER BY dst_item")?;
    let written: Vec<String> = written_stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let mut resolver = CodeResolver::new(root, conn);
    let mut ids = BTreeSet::new();
    for dst in written {
        let hits = match resolver.resolve(dst.strip_prefix("code:").unwrap_or(&dst))? {
            CodeLink::File(p) | CodeLink::Range { path: p, .. } => p == path,
            CodeLink::Symbol(qname) => conn
                .prepare_cached(
                    "SELECT 1 FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.qname = ?1 AND f.path = ?2 LIMIT 1",
                )?
                .exists([&qname, path])?,
            _ => false,
        };
        if hits {
            let mut stmt =
                conn.prepare_cached("SELECT DISTINCT src_item FROM links WHERE kind = 'code' AND dst_item = ?1")?;
            for id in stmt.query_map([&dst], |row| row.get::<_, String>(0))? {
                ids.insert(id?);
            }
        }
    }
    Ok(ids.into_iter().collect())
}
