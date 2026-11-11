//! `stone history`: the commits that touched an item's file, newest first,
//! each with its diff of that file. With `#anchor` only the commits whose
//! diff touches that section stay.
//!
//! A task's work happens elsewhere: its file may never be committed while the code repos hold
//! commits that name it. So a task also lists the commits tied to it (see `work_links`), without a
//! diff, since those show the code and not the task.

use std::collections::HashSet;

use carvestone_core::{ItemType, sections};
use carvestone_git::{Commit, Git};
use carvestone_store::Document;
use serde::Serialize;

use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::work_links::{LinkHow, WorkLinks};

/// Diff lines shown per commit before the cut.
pub const DIFF_LINES: usize = 60;
/// Commits read when an anchor filters them, so a long history stays bounded.
const ANCHOR_SCAN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryOpts {
    pub limit: usize,
    pub diff: bool,
}

impl Default for HistoryOpts {
    fn default() -> Self {
        HistoryOpts { limit: 10, diff: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistoryEntry {
    pub sha: String,
    pub date: String,
    pub author: String,
    pub subject: String,
    /// The values of the commit's `Refs` trailers.
    pub refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub diff: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct History {
    /// The item as asked for, with its `#anchor` when one was given.
    pub id: String,
    pub entries: Vec<HistoryEntry>,
    /// Older commits of the file the limit left out (at most the 500 that are read). With an
    /// anchor they are not checked, so some may not touch the section.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub older: usize,
    #[serde(skip)]
    pub filtered: bool,
}

impl History {
    pub fn text(&self) -> String {
        if self.entries.is_empty() && self.older == 0 {
            return if self.filtered {
                format!("no commit touches {}\n", self.id)
            } else {
                format!("{} has no commits yet\n", self.id)
            };
        }
        let mut out = String::new();
        for e in &self.entries {
            let day = e.date.get(..10).unwrap_or(&e.date);
            out.push_str(&format!(
                "{}  {day}  {}  {}\n",
                e.sha.get(..7).unwrap_or(&e.sha),
                e.author,
                e.subject
            ));
            if let Some(diff) = &e.diff
                && !diff.is_empty()
            {
                out.push_str(diff);
                if !diff.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
        }
        if self.older > 0 {
            let s = if self.older == 1 { "" } else { "s" };
            let what = if self.filtered { "not checked" } else { "left out" };
            out.push_str(&format!("{} older commit{s} {what}; raise --limit\n", self.older));
        }
        out
    }
}

pub fn history(repo: &Repo, input: &str, opts: &HistoryOpts) -> Result<History, EngineError> {
    let snapshot = repo.snapshot()?;
    let mut found = file_history(repo, &snapshot, input, opts)?;
    let (entry, anchor) = snapshot.resolve(input)?;
    let is_task = entry.item.as_ref().and_then(|item| item.item_type()) == Some(ItemType::Task);
    if is_task && anchor.is_none() {
        let wanted = if opts.limit == 0 { 10 } else { opts.limit };
        add_task_commits(
            &WorkLinks::load(repo, &snapshot)?,
            entry.id.as_str(),
            &snapshot,
            &mut found,
            wanted,
        );
    }
    Ok(found)
}

/// Puts the commits tied to the task among the commits of its file, newest first.
fn add_task_commits(links: &WorkLinks, id: &str, snapshot: &Snapshot, found: &mut History, wanted: usize) {
    let have: HashSet<String> = found.entries.iter().map(|e| e.sha.clone()).collect();
    let label = snapshot
        .entries
        .iter()
        .find(|e| e.id.as_str() == id)
        .map_or_else(|| id.to_string(), |e| snapshot.label(e));
    found.entries.extend(
        links
            .commits_of(id)
            .into_iter()
            .filter(|commit| !have.contains(&commit.sha))
            .map(|commit| HistoryEntry {
                refs: if commit.how == LinkHow::Refs {
                    vec![label.clone()]
                } else {
                    Vec::new()
                },
                sha: commit.sha,
                date: commit.date,
                author: commit.author,
                subject: commit.title,
                diff: None,
            }),
    );
    found.entries.sort_by(|a, b| b.date.cmp(&a.date));
    found.older += found.entries.len().saturating_sub(wanted);
    found.entries.truncate(wanted);
}

/// The commits of the item's own file.
fn file_history(repo: &Repo, snapshot: &Snapshot, input: &str, opts: &HistoryOpts) -> Result<History, EngineError> {
    let (entry, anchor) = snapshot.resolve(input)?;
    let anchor = match anchor {
        Some(a) => Some(snapshot.find_section(entry, &a)?.anchor),
        None => None,
    };
    let label = snapshot.label(entry);
    let id = match &anchor {
        Some(a) => format!("{label}#{a}"),
        None => label,
    };

    // The item's file lives in the repo that holds `stone/`; git speaks in that repo's paths.
    // An item in no repo has no history to show.
    let repos = repo.repos();
    let Some((one, inner)) = repos.repo_of(&entry.path) else {
        return Ok(History {
            id,
            entries: Vec::new(),
            older: 0,
            filtered: false,
        });
    };
    let git = one.git();
    let wanted = if opts.limit == 0 { 10 } else { opts.limit };
    // Commits are cheap to list; only the ones shown get a diff, so the rest can be counted.
    let commits = git.file_log(&inner, true, ANCHOR_SCAN.max(wanted + 1))?;
    let total = commits.len();

    let mut entries = Vec::new();
    let mut read = 0;
    // The file's name in the commit being read; it changes going back past a rename.
    let mut name = inner;
    for commit in commits {
        read += 1;
        let before = renamed_from(&commit, &name);
        let paths: Vec<&str> = std::iter::once(name.as_str()).chain(before.as_deref()).collect();
        let raw = git.show_diff(&commit.sha, &paths)?;
        let patch = hunks_of(&raw, before.as_deref());
        if let Some(anchor) = &anchor
            && !touches_section(git, &commit.sha, &name, &patch, anchor)?
        {
            name = before.unwrap_or(name);
            continue;
        }
        entries.push(HistoryEntry {
            sha: commit.sha.clone(),
            date: commit.date.clone(),
            author: commit.author.clone(),
            subject: commit.subject.clone(),
            refs: commit
                .trailers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("refs"))
                .map(|(_, v)| v.clone())
                .collect(),
            diff: opts.diff.then(|| cut(&patch)),
        });
        name = before.unwrap_or(name);
        if entries.len() == wanted {
            break;
        }
    }
    Ok(History {
        id,
        entries,
        older: total - read,
        filtered: anchor.is_some(),
    })
}

/// The path the commit renamed `name` from, when it did.
fn renamed_from(commit: &Commit, name: &str) -> Option<String> {
    commit
        .files
        .iter()
        .find(|f| f.path == name && matches!(f.status, 'R' | 'C'))
        .and_then(|f| f.old_path.clone())
        .filter(|old| old != name)
}

/// The patch from its first hunk on; a pure rename has no hunk and says so.
fn hunks_of(raw: &str, renamed_from: Option<&str>) -> String {
    let start = raw
        .lines()
        .scan(0usize, |at, line| {
            let here = *at;
            *at += line.len() + 1;
            Some((here, line))
        })
        .find(|(_, line)| line.starts_with("@@ "))
        .map(|(at, _)| at);
    match (start, renamed_from) {
        (Some(at), _) => raw[at..].to_string(),
        (None, Some(old)) => format!("renamed from {old}\n"),
        (None, None) => String::new(),
    }
}

/// The first `DIFF_LINES` lines, then how many were left out.
fn cut(patch: &str) -> String {
    let lines: Vec<&str> = patch.lines().collect();
    if lines.len() <= DIFF_LINES {
        return patch.to_string();
    }
    let mut out = lines[..DIFF_LINES].join("\n");
    out.push_str(&format!("\n… {} more lines\n", lines.len() - DIFF_LINES));
    out
}

/// Whether the commit's patch changes the section `anchor` of the file as the commit left it.
fn touches_section(git: &Git, sha: &str, name: &str, patch: &str, anchor: &str) -> Result<bool, EngineError> {
    let Some(text) = git.file_at(sha, name)? else {
        return Ok(false);
    };
    let Ok(doc) = Document::parse(&text) else {
        return Ok(false);
    };
    let found = sections(&doc.body, doc.body_line());
    let Some(section) = found.iter().find(|s| s.anchor == anchor) else {
        return Ok(false);
    };
    Ok(hunks_touch(patch, section.start_line, section.end_line))
}

/// Whether a hunk adds a line inside `start..=end` of the new file, or removes
/// one from inside it. A removal sits just before the new line it is read at.
fn hunks_touch(patch: &str, start: usize, end: usize) -> bool {
    let mut new_line = 0usize;
    let mut in_hunk = false;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("@@ ") {
            let Some(plus) = rest.split(" +").nth(1) else {
                return false;
            };
            let mut numbers = plus.split([',', ' ']);
            new_line = numbers.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            // A hunk that adds nothing names the line before the removal.
            if numbers.next() == Some("0") {
                new_line += 1;
            }
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        match line.chars().next() {
            Some('+') => {
                if (start..=end).contains(&new_line) {
                    return true;
                }
                new_line += 1;
            }
            Some('-') => {
                if new_line > start && new_line <= end + 1 {
                    return true;
                }
            }
            Some(' ') => new_line += 1,
            _ => {}
        }
    }
    false
}
