//! Which commits belong to which task, and when a task was closed.
//!
//! A task's file says little about its work: the commits live in the code repos and most of them
//! name no task. A commit joins a task by the first of four routes that fits, each one weaker
//! than the one before: a `Refs` trailer, the id in the message, its sha quoted in the task's
//! `## Outcome`, or a file of the task's `paths` changed between the task's `created` day and its
//! end. The last route is a guess, so it asks for more: the task has that first day and has been
//! started, nothing names the commit elsewhere, and no other task's files claim the commit too.

use std::collections::{BTreeMap, HashMap, HashSet};

use carvestone_core::{Item, ItemType, NumberedKind, section_text, sections};
use carvestone_store::{Config, Entry};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::create::today;
use crate::docs_list::updated_days;
use crate::git_cache::index_with_history;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::stale::day_of;

/// The shortest piece of a sha worth matching, as `git log --oneline` prints at least.
const MIN_SHA: usize = 7;
const FULL_SHA: usize = 40;

/// How a commit was tied to a task, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LinkHow {
    /// A `Refs` trailer names the task.
    Refs,
    /// The commit message names the task.
    Message,
    /// The task's `## Outcome` quotes the sha.
    Outcome,
    /// The commit changed a file of the task's `paths` after the task was created and before it
    /// ended, and no other task claims those files for it.
    Path,
}

/// One commit of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TaskCommit {
    pub sha: String,
    /// The repo folder relative to the product root; empty for the root repo.
    pub repo: String,
    /// The commit's author date, ISO 8601.
    pub date: String,
    pub title: String,
    pub how: LinkHow,
    #[serde(skip)]
    pub author: String,
}

/// The day a task was closed. `estimated` when the task records no `closed` day and this one is
/// read from its newest commit or the last change of its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    pub day: String,
    pub estimated: bool,
}

struct CommitRow {
    repo: String,
    sha: String,
    date: String,
    author: String,
    subject: String,
}

impl CommitRow {
    fn day(&self) -> &str {
        self.date.get(..10).unwrap_or(&self.date)
    }
}

/// The commits of every task, read once for a whole page.
pub(crate) struct WorkLinks {
    /// Every commit of every repo, newest first.
    commits: Vec<CommitRow>,
    /// By task id: positions in `commits` and how each joined, so newest first.
    by_task: HashMap<String, BTreeMap<usize, LinkHow>>,
    /// The last change of every store file, by path.
    updated: BTreeMap<String, String>,
    /// The project's statuses of work that is over.
    finished: Vec<String>,
}

/// The task items of the store.
pub(crate) fn tasks(snapshot: &Snapshot) -> impl Iterator<Item = (&Entry, &Item)> {
    snapshot.entries.iter().filter_map(|entry| {
        let item = entry.item.as_ref()?;
        (item.item_type() == Some(ItemType::Task)).then_some((entry, item))
    })
}

/// A store or code path without its `#Symbol` and trailing slash.
pub(crate) fn clean_path(path: &str) -> &str {
    path.split('#').next().unwrap_or(path).trim().trim_end_matches('/')
}

fn read_commits(conn: &Connection) -> Result<Vec<CommitRow>, EngineError> {
    let mut stmt = conn.prepare("SELECT repo, sha, date, author, subject FROM commits ORDER BY ts DESC, sha, repo")?;
    let rows = stmt.query_map([], |row| {
        Ok(CommitRow {
            repo: row.get(0)?,
            sha: row.get(1)?,
            date: row.get(2)?,
            author: row.get(3)?,
            subject: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The numbers of the ids `PREFIX-n` standing as words in `text`.
fn named_numbers(text: &str, prefix: &str) -> Vec<u32> {
    let wanted = format!("{prefix}-");
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut out = Vec::new();
    for (at, _) in text.match_indices(&wanted) {
        if text[..at].chars().next_back().is_some_and(is_word) {
            continue;
        }
        let rest = &text[at + wanted.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if rest[end..].chars().next().is_some_and(char::is_alphanumeric) {
            continue;
        }
        if let Ok(number) = rest[..end].parse() {
            out.push(number);
        }
    }
    out
}

/// The pieces of `text` that could be a sha, in lower case.
fn sha_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| (MIN_SHA..=FULL_SHA).contains(&word.len()) && word.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The text of the task's `## Outcome`, when it has one.
fn outcome_of(item: &Item) -> Option<&str> {
    sections(&item.body, item.body_line)
        .iter()
        .find(|section| section.title.eq_ignore_ascii_case("outcome"))
        .map(|section| section_text(&item.body, item.body_line, section))
}

/// The first and last day a commit on a task's files can be its work. `None` when that cannot be
/// told: the task was not started, it has no `created` day, or that day comes after its last day,
/// as on an imported task.
fn open_days(
    config: &Config,
    entry: &Entry,
    item: &Item,
    updated: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    let status = item.fields.status.as_deref().unwrap_or_default();
    if !config.task_started(status) {
        return None;
    }
    let recorded = item.fields.closed.as_deref().and_then(day_of).map(|day| day.iso());
    let last = recorded.or_else(|| {
        config
            .task_finished(status)
            .then(|| updated.get(&entry.path).cloned())
            .flatten()
    });
    let last = last.unwrap_or_else(today);
    let first = item.fields.created.as_deref().and_then(day_of)?.iso();
    (first <= last).then_some((first, last))
}

/// The commits that changed `path`, a file or a folder.
fn commits_on(conn: &Connection, path: &str) -> Result<Vec<(String, String)>, EngineError> {
    // `0` is the character after `/`, so the second test is every path below the folder.
    let mut stmt = conn.prepare_cached(
        "SELECT repo, sha FROM commit_files WHERE path = ?1 OR (path >= ?1 || '/' AND path < ?1 || '0')",
    )?;
    let rows = stmt.query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

impl WorkLinks {
    pub(crate) fn load(repo: &Repo, snapshot: &Snapshot) -> Result<WorkLinks, EngineError> {
        let mut links = WorkLinks {
            commits: Vec::new(),
            by_task: HashMap::new(),
            updated: updated_days(repo)?,
            finished: repo.store.config.finished_statuses().to_vec(),
        };
        let Some(index) = index_with_history(repo)? else {
            return Ok(links);
        };
        let conn = index.conn();
        links.commits = read_commits(conn)?;
        let at: HashMap<(&str, &str), usize> = links
            .commits
            .iter()
            .enumerate()
            .map(|(n, c)| ((c.repo.as_str(), c.sha.as_str()), n))
            .collect();
        let mut found: HashMap<String, BTreeMap<usize, LinkHow>> = HashMap::new();
        // Routes run strongest first; a commit keeps the first route that reached it.
        let mut join = |task: &str, commit: usize, how: LinkHow| {
            found.entry(task.to_string()).or_default().entry(commit).or_insert(how);
        };

        let ids: HashSet<&str> = tasks(snapshot).map(|(entry, _)| entry.id.as_str()).collect();
        let mut stmt = conn.prepare("SELECT repo, sha, ref_id FROM commit_refs WHERE kind = 'task'")?;
        let refs = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in refs {
            let (from, sha, id) = row?;
            if let Some(n) = at.get(&(from.as_str(), sha.as_str()))
                && ids.contains(id.as_str())
            {
                join(&id, *n, LinkHow::Refs);
            }
        }

        let numbers: HashMap<u32, &str> = tasks(snapshot)
            .filter_map(|(entry, _)| match snapshot.scheme.number(&entry.id) {
                Some((NumberedKind::Task, n)) => Some((n, entry.id.as_str())),
                _ => None,
            })
            .collect();
        let prefix = &repo.store.config.task_prefix;
        for one in repo.repos().all() {
            // A repo git cannot read is reported by the cache fill; the others still answer.
            let Ok(messages) = one.git().messages_matching(&format!("{prefix}-[0-9]+")) else {
                continue;
            };
            for (sha, message) in messages {
                let Some(n) = at.get(&(one.dir(), sha.as_str())) else {
                    continue;
                };
                for number in named_numbers(&message, prefix) {
                    if let Some(id) = numbers.get(&number) {
                        join(id, *n, LinkHow::Message);
                    }
                }
            }
        }

        let mut shas: Vec<(&str, usize)> = links
            .commits
            .iter()
            .enumerate()
            .map(|(n, c)| (c.sha.as_str(), n))
            .collect();
        shas.sort_unstable();
        for (entry, item) in tasks(snapshot) {
            for word in outcome_of(item).map(sha_words).unwrap_or_default() {
                let from = shas.partition_point(|(sha, _)| *sha < word.as_str());
                for (_, n) in shas[from..].iter().take_while(|(sha, _)| sha.starts_with(&word)) {
                    join(entry.id.as_str(), *n, LinkHow::Outcome);
                }
            }
        }

        let claimed: HashSet<usize> = found.values().flat_map(|commits| commits.keys().copied()).collect();
        let mut wanted: BTreeMap<usize, HashSet<&str>> = BTreeMap::new();
        for (entry, item) in tasks(snapshot) {
            let Some((first, last)) = open_days(&repo.store.config, entry, item, &links.updated) else {
                continue;
            };
            for path in item.fields.paths.iter().map(|p| clean_path(p)) {
                for (from, sha) in commits_on(conn, path)? {
                    let Some(n) = at.get(&(from.as_str(), sha.as_str())).copied() else {
                        continue;
                    };
                    let day = links.commits[n].day();
                    if day >= first.as_str() && day <= last.as_str() && !claimed.contains(&n) {
                        wanted.entry(n).or_default().insert(entry.id.as_str());
                    }
                }
            }
        }
        // A commit two tasks could claim belongs to neither for sure, so it joins no task.
        for (n, claimants) in wanted {
            if let [task] = claimants.into_iter().collect::<Vec<_>>()[..] {
                found.entry(task.to_string()).or_default().insert(n, LinkHow::Path);
            }
        }
        links.by_task = found;
        Ok(links)
    }

    /// The commits of the task with this item id, newest first.
    pub(crate) fn commits_of(&self, id: &str) -> Vec<TaskCommit> {
        let Some(linked) = self.by_task.get(id) else {
            return Vec::new();
        };
        linked
            .iter()
            .map(|(n, how)| {
                let row = &self.commits[*n];
                TaskCommit {
                    sha: row.sha.clone(),
                    repo: row.repo.clone(),
                    date: row.date.clone(),
                    title: row.subject.clone(),
                    how: *how,
                    author: row.author.clone(),
                }
            })
            .collect()
    }

    /// The last change of a store file.
    pub(crate) fn updated(&self, path: &str) -> Option<&str> {
        self.updated.get(path).map(String::as_str)
    }

    /// When a finished task was closed: its `closed` field, which an archived task keeps. A done
    /// task with no such field estimates it from its newest commit, else the last change of its
    /// file. `None` for a task that is not finished.
    pub(crate) fn closed(&self, entry: &Entry, item: &Item) -> Option<Closed> {
        let status = item.fields.status.as_deref()?;
        if !self.finished.iter().any(|known| known == status) {
            return None;
        }
        if let Some(day) = item.fields.closed.as_deref().and_then(day_of) {
            return Some(Closed {
                day: day.iso(),
                estimated: false,
            });
        }
        if status != "done" {
            return None;
        }
        let newest = self
            .by_task
            .get(entry.id.as_str())
            .and_then(|linked| linked.keys().next())
            .map(|n| self.commits[*n].day());
        newest.or_else(|| self.updated(&entry.path)).map(|day| Closed {
            day: day.to_string(),
            estimated: true,
        })
    }
}

/// The commits of a task, newest first; the answer of `GET /api/tasks/{id}/commits`.
pub fn task_commits(repo: &Repo, input: &str) -> Result<Vec<TaskCommit>, EngineError> {
    let snapshot = repo.snapshot()?;
    let (entry, _) = snapshot.resolve(input)?;
    if entry.item.as_ref().and_then(Item::item_type) != Some(ItemType::Task) {
        return Err(EngineError::NotATask {
            id: snapshot.label(entry),
        });
    }
    Ok(WorkLinks::load(repo, &snapshot)?.commits_of(entry.id.as_str()))
}
