//! The files a task changed: the files of every commit whose `Refs` trailer names it, and, while
//! the task is in progress, the working-tree changes that plausibly belong to it.

use std::collections::HashSet;

use carvestone_core::ItemType;
use carvestone_store::Entry;
use globset::GlobSet;
use rusqlite::Connection;
use serde::Serialize;

use crate::git_cache::index_with_history;
use crate::owner::arch_elements;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::why::{covers, names};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChangedFile {
    pub path: String,
    /// `A`, `M`, `D`, `R`, `C`, `T` from a commit; also `?` for an untracked file in the working tree.
    pub status: char,
    /// `commit` or `working`.
    pub source: String,
    /// The newest commit that touched the file; left out for working changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub sha: Option<String>,
}

impl ChangedFile {
    /// `M  src/a.cs  1a2b3c4`, or `working` in place of the sha.
    pub fn line(&self) -> String {
        let at = match &self.sha {
            Some(sha) => sha.get(..7).unwrap_or(sha),
            None => "working",
        };
        format!("{}  {}  {at}", self.status, self.path)
    }
}

/// The `Changed files:` block `stone get` prints after the body, or nothing for an empty list.
pub fn changed_files_block(files: &[ChangedFile]) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let mut out = String::from("\nChanged files:\n");
    for file in files {
        out.push_str(&file.line());
        out.push('\n');
    }
    Some(out)
}

/// The changed files of the task `input`: empty for anything but a task, and without git.
pub fn changed_files(repo: &Repo, input: &str) -> Result<Vec<ChangedFile>, EngineError> {
    let snapshot = repo.snapshot()?;
    let (entry, _) = snapshot.resolve(input)?;
    if !is_task(entry) {
        return Ok(Vec::new());
    }
    let Some(index) = index_with_history(repo)? else {
        return Ok(Vec::new());
    };
    changed_in(repo, &snapshot, entry, index.conn())
}

fn is_task(entry: &Entry) -> bool {
    entry.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Task)
}

/// `changed_files` for a task entry, over a cache whose history is already whole.
pub(crate) fn changed_in(
    repo: &Repo,
    snapshot: &Snapshot,
    entry: &Entry,
    conn: &Connection,
) -> Result<Vec<ChangedFile>, EngineError> {
    let Some(item) = &entry.item else {
        return Ok(Vec::new());
    };
    // Newest commit first, so the first sighting of a path carries the newest status and sha.
    let mut stmt = conn.prepare_cached(
        "SELECT f.path, f.status, c.sha FROM commit_refs r \
         JOIN commits c ON c.repo = r.repo AND c.sha = r.sha \
         JOIN commit_files f ON f.repo = c.repo AND f.sha = c.sha \
         WHERE r.kind = 'task' AND r.ref_id = ?1 ORDER BY c.ts DESC, c.sha ASC, c.repo ASC",
    )?;
    let rows = stmt.query_map([entry.id.as_str()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for row in rows {
        let (path, status, sha) = row?;
        if seen.insert(path.clone()) {
            files.push(ChangedFile {
                path,
                status: status.chars().next().unwrap_or('M'),
                source: "commit".into(),
                sha: Some(sha),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    if item.fields.status.as_deref() == Some("in-progress") {
        files.extend(working(repo, snapshot, entry)?);
    }
    Ok(files)
}

/// Uncommitted changes outside `stone/`, limited to the task's `paths` and its components' code
/// globs when it names either. A git problem here means no working files, not a failed `get`.
fn working(repo: &Repo, snapshot: &Snapshot, entry: &Entry) -> Result<Vec<ChangedFile>, EngineError> {
    let Some(item) = &entry.item else {
        return Ok(Vec::new());
    };
    // Every repo reports its own tree; a path a deeper repo holds is left to that repo.
    let repos = repo.repos();
    let mut changes = Vec::new();
    for one in repos.all() {
        let Ok(found) = one.git().working_changes(&[]) else {
            continue;
        };
        for mut change in found {
            change.path = repos.to_root(one, &change.path);
            if repos
                .repo_of(&change.path)
                .is_some_and(|(owner, _)| owner.dir() == one.dir())
            {
                changes.push(change);
            }
        }
    }
    let limited = !item.fields.paths.is_empty() || !item.fields.components.is_empty();
    let elements = arch_elements(snapshot, &mut Vec::new());
    let globs: Vec<&GlobSet> = elements
        .iter()
        .filter(|e| names(snapshot, &item.fields.components, &e.id))
        .map(|e| &e.globs)
        .collect();
    Ok(changes
        .into_iter()
        .filter(|c| !c.path.starts_with("stone/") && !c.path.starts_with(".stone/"))
        .filter(|c| !limited || covers(&item.fields.paths, &c.path) || globs.iter().any(|g| g.is_match(&c.path)))
        .map(|c| ChangedFile {
            path: c.path,
            status: c.status,
            source: "working".into(),
            sha: None,
        })
        .collect())
}
