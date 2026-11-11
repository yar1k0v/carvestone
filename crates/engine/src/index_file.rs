//! The root `index.md`: one line per item with its description, kept in step with every write.
//!
//! Stone owns the line of each item and nothing else: a line it did not write stays as it is.

use std::collections::HashSet;

use carvestone_core::{IdScheme, Item, ItemId, Link, Section, eol, links, sections};
use carvestone_store::lock::WriteLock;

use crate::create::{INDEX, Written};
use crate::docs_list::summary_of;
use crate::remove::few;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

const ROOT_INDEX: &str = "stone/index.md";

/// The folder of each section and its heading, in the order the sections appear.
const SECTIONS: [(&str, &str); 7] = [
    ("docs", "Docs"),
    ("arch", "Architecture"),
    ("decisions", "Decisions"),
    ("plans", "Plans"),
    ("milestones", "Milestones"),
    ("tasks", "Tasks"),
    ("refs", "References"),
];

/// Numbered items sort by number, the others by id as bytes.
type Key = (bool, u32, String);

fn folder_rank(folder: &str) -> Option<usize> {
    SECTIONS.iter().position(|(name, _)| *name == folder)
}

fn rank_of(id: &ItemId) -> Option<usize> {
    folder_rank(id.as_str().split('/').next().unwrap_or(""))
}

fn key_of(scheme: &IdScheme, id: &ItemId) -> Key {
    match scheme.number(id) {
        Some((_, n)) => (false, n, id.to_string()),
        None => (true, 0, id.to_string()),
    }
}

/// What the link says: `CS-1: First` for a numbered item, the title for the others.
pub(crate) fn link_text(scheme: &IdScheme, id: &ItemId, item: &Item) -> String {
    let title = item
        .fields
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| id.as_str().rsplit('/').next().unwrap_or(id.as_str()))
        .replace('[', "(")
        .replace(']', ")");
    match scheme.short(id) {
        Some(label) => format!("{label}: {title}"),
        None => title,
    }
}

/// The whole line of an item, without a line ending.
pub(crate) fn line_of(scheme: &IdScheme, id: &ItemId, item: &Item) -> String {
    let mut line = format!("- [{}]({id}.md)", link_text(scheme, id, item));
    let description = item
        .fields
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .or_else(|| summary_of(item));
    if let Some(description) = description {
        let flat = description.split_whitespace().collect::<Vec<_>>().join(" ");
        if !flat.is_empty() {
            line.push_str(" - ");
            line.push_str(&flat);
        }
    }
    line
}

fn split(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(str::to_string).collect()
}

fn ending(line: &str) -> &'static str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

fn bare(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

/// The first link of every line that has one.
fn first_links(index: &str) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    for link in links(index, 1) {
        if out.last().is_none_or(|last| last.line != link.line) {
            out.push(link);
        }
    }
    out
}

/// The 1-based line whose first link has exactly this target.
fn line_for(index: &str, target: &str) -> Option<usize> {
    first_links(index)
        .into_iter()
        .find(|l| l.target == target)
        .map(|l| l.line)
}

/// The top-level sections stone writes, by rank; the first one of a name wins.
fn ours(all: &[Section]) -> Vec<(usize, &Section)> {
    (0..SECTIONS.len())
        .filter_map(|rank| {
            all.iter()
                .find(|s| s.level == 1 && s.title.trim().eq_ignore_ascii_case(SECTIONS[rank].1))
                .map(|s| (rank, s))
        })
        .collect()
}

/// A line ends with a line ending before something follows it.
fn close_line(lines: &mut [String], at: usize, eol: &str) {
    if let Some(line) = lines.get_mut(at)
        && ending(line).is_empty()
    {
        line.push_str(eol);
    }
}

/// Sets the item's line: replaced in place when the file has one, else added to its section in order.
pub(crate) fn put(index: &str, scheme: &IdScheme, id: &ItemId, item: &Item) -> String {
    let Some(rank) = rank_of(id) else {
        return index.to_string();
    };
    let new_line = line_of(scheme, id, item);
    let mut lines = split(index);
    if let Some(at) = line_for(index, &format!("{id}.md")).map(|n| n - 1)
        && let Some(old) = lines.get_mut(at)
    {
        *old = format!("{new_line}{}", ending(old));
        return lines.concat();
    }

    let eol = eol(index);
    let all = sections(index, 1);
    let mine = ours(&all);
    let Some((_, section)) = mine.iter().find(|(r, _)| *r == rank) else {
        let heading = format!("# {}{eol}{eol}{new_line}{eol}", SECTIONS[rank].1);
        if let Some((_, later)) = mine.iter().filter(|(r, _)| *r > rank).min_by_key(|(r, _)| *r) {
            lines.insert(later.start_line - 1, format!("{heading}{eol}"));
            return lines.concat();
        }
        let mut out = index.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        if !(out.is_empty() || out.ends_with("\n\n") || out.ends_with("\n\r\n")) {
            out.push_str(eol);
        }
        out.push_str(&heading);
        return out;
    };

    let new_key = key_of(scheme, id);
    let items: Vec<Link> = first_links(index)
        .into_iter()
        .filter(|l| l.line > section.start_line && l.line <= section.end_line && l.is_relative_md())
        .collect();
    let line = format!("{new_line}{eol}");
    let after = |l: &Link| {
        let path = l.target.split_once('#').map_or(l.target.as_str(), |(p, _)| p);
        key_of(scheme, &ItemId::from_path(path)) > new_key
    };
    if let Some(next) = items.iter().find(|l| after(l)) {
        lines.insert(next.line - 1, line);
    } else if let Some(last) = items.last() {
        close_line(&mut lines, last.line - 1, eol);
        lines.insert(last.line, line);
    } else {
        let end = section.end_line.min(lines.len());
        let last = (section.start_line..=end)
            .rev()
            .find(|n| !bare(&lines[n - 1]).trim().is_empty())
            .unwrap_or(section.start_line);
        close_line(&mut lines, last - 1, eol);
        if last == section.start_line {
            // A blank line already under the heading is the one the item needs.
            if last < end && bare(&lines[last]).trim().is_empty() {
                lines.insert(last + 1, line);
            } else {
                lines.insert(last, eol.to_string());
                lines.insert(last + 1, line);
            }
        } else {
            lines.insert(last, line);
        }
    }
    lines.concat()
}

/// Removes the item's line with its line ending; every other line stays.
pub(crate) fn drop_line(index: &str, id: &ItemId) -> String {
    let Some(n) = line_for(index, &format!("{id}.md")) else {
        return index.to_string();
    };
    let mut lines = split(index);
    if n <= lines.len() {
        lines.remove(n - 1);
    }
    lines.concat()
}

/// The targets of the lines that name a file in one of the item folders that no item has.
fn dangling(index: &str, snapshot: &Snapshot) -> Vec<String> {
    let known: HashSet<&str> = snapshot.entries.iter().map(|e| e.id.as_str()).collect();
    first_links(index)
        .into_iter()
        .filter(|l| l.is_relative_md() && !l.target.contains('#'))
        .filter(|l| {
            let mut parts = l.target.split('/');
            parts.next().and_then(folder_rank).is_some()
                && l.target.contains('/')
                && !l.target.split('/').any(|s| matches!(s, "" | "." | ".."))
        })
        .map(|l| l.target)
        .filter(|target| !known.contains(target.strip_suffix(".md").unwrap_or(target)))
        .collect()
}

/// The files the index is wrong about, sorted: an item whose line is missing or differs, and a
/// file named in a line that no item has.
pub(crate) fn stale(index: &str, snapshot: &Snapshot) -> Vec<String> {
    let lines = split(index);
    let firsts = first_links(index);
    let mut out: Vec<String> = Vec::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        if rank_of(&entry.id).is_none() {
            continue;
        }
        let want = line_of(&snapshot.scheme, &entry.id, item);
        let target = format!("{}.md", entry.id);
        let have = firsts
            .iter()
            .find(|l| l.target == target)
            .and_then(|l| lines.get(l.line - 1))
            .map(|l| bare(l));
        if have != Some(want.as_str()) {
            out.push(entry.path.clone());
        }
    }
    out.extend(dangling(index, snapshot).into_iter().map(|t| format!("stone/{t}")));
    out.sort();
    out.dedup();
    out
}

impl Repo {
    /// Changes the root index inside the write that changed an item. Nothing is written when the
    /// change leaves the text as it was, so an item outside the folders never creates the file.
    pub(crate) fn sync_index(&self, lock: &WriteLock, change: impl FnOnce(&str) -> String) -> Result<(), EngineError> {
        let text = self.store.read_index()?.unwrap_or_else(|| INDEX.to_string());
        let next = change(&text);
        if next != text {
            self.store.save_index(lock, &next)?;
        }
        Ok(())
    }

    /// Writes the line of every item and removes the lines of files that are gone. A dry run takes
    /// no lock and writes nothing.
    pub fn write_index(&self, dry_run: bool) -> Result<Written, EngineError> {
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        let snapshot = self.snapshot()?;
        let current = self.store.read_index()?;
        let mut text = current.clone().unwrap_or_else(|| INDEX.to_string());
        for target in dangling(&text, &snapshot) {
            text = drop_line(&text, &ItemId::from_path(&target));
        }
        for entry in &snapshot.entries {
            if let Some(item) = &entry.item {
                text = put(&text, &snapshot.scheme, &entry.id, item);
            }
        }
        let changed = current.as_deref() != Some(text.as_str());
        if changed && let Some(lock) = &lock {
            self.store.save_index(lock, &text)?;
        }
        Ok(Written {
            id: ROOT_INDEX.to_string(),
            path: ROOT_INDEX.to_string(),
            changed,
            text,
            warnings: Vec::new(),
        })
    }
}

/// What `stone check` says about a stale index.
pub(crate) fn stale_message(stale: &[String]) -> String {
    format!("the index is out of date for {}; run `stone index`", few(stale))
}
