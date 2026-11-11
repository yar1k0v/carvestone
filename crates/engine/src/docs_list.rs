//! The docs as a reader meets them: what each one is, what it says in a sentence and when it last
//! changed, so a library page can be browsed without opening every file.

use std::collections::{BTreeMap, HashMap, HashSet};

use carvestone_core::{Item, ItemType, first_paragraph, lines, section_text, sections};
use carvestone_store::Entry;
use rusqlite::Connection;
use serde::Serialize;

use crate::git_cache::index_with_history;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::timeline::Date;

/// Section titles that say in a few sentences what a doc is about, best first.
const SUMMARY_SECTIONS: [&str; 7] = [
    "in plain words",
    "summary",
    "overview",
    "purpose",
    "purpose and boundaries",
    "what this is",
    "what it is",
];
/// A summary is cut at the end of a word past this many characters.
const SUMMARY_CHARS: usize = 220;
const WORDS_PER_MINUTE: usize = 200;
/// A name shared by fewer docs than this is no group; they read as `other`.
const MIN_GROUP: usize = 3;

/// One doc of the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DocRow {
    pub id: String,
    pub title: String,
    /// The family the doc belongs to: its folder, else the first word of its name (`lld`,
    /// `feature`, `runbook`); `other` when too few others share it.
    pub group: String,
    /// What the doc calls itself (`flow`, `component`), from `kind` or a `Kind:` line.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub summary: Option<String>,
    /// `YYYY-MM-DD` of the last change: the file's newest commit or a later edit on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub updated: Option<String>,
    /// Minutes to read it, at least one.
    pub minutes: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,
    /// A blank to copy, not a document to read.
    pub template: bool,
}

/// The first word of a doc's name, or its folder: `docs/lld-web-map` and `docs/lld/web` are `lld`.
pub(crate) fn family(id: &str) -> &str {
    let name = id.strip_prefix("docs/").unwrap_or(id);
    match name.split_once('/') {
        Some((folder, _)) => folder,
        None => name.split('-').next().unwrap_or(name),
    }
}

/// True for a doc written at the low level: marked `level: lld`, or kept in the `lld` family.
pub(crate) fn is_lld(id: &str, item: &Item) -> bool {
    match item.fields.level.as_deref() {
        Some(level) => level == "lld",
        None => family(id) == "lld",
    }
}

/// What the doc calls itself: its `kind` field, else `Kind: x` near the top of the body.
pub(crate) fn kind_of(item: &Item) -> Option<String> {
    if let Some(kind) = item.fields.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        return Some(kind.to_string());
    }
    lines(&item.body, item.body_line)
        .into_iter()
        .filter(|(_, _, in_fence)| !in_fence)
        .take(6)
        .find_map(|(_, text, _)| {
            let rest = text.trim().strip_prefix("Kind:")?;
            let word = rest.split('·').next()?.trim().to_lowercase();
            (!word.is_empty()).then_some(word)
        })
}

fn cut(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= SUMMARY_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(SUMMARY_CHARS).collect();
    let end = head.rfind(' ').unwrap_or(head.len());
    format!("{}…", head[..end].trim_end_matches([',', ';', ':']))
}

/// What the doc says about itself: the section meant for that, else its first paragraph.
pub(crate) fn summary_of(item: &Item) -> Option<String> {
    let all = sections(&item.body, item.body_line);
    let wanted = SUMMARY_SECTIONS
        .iter()
        .find_map(|name| all.iter().find(|section| section.title.eq_ignore_ascii_case(name)));
    wanted
        .and_then(|section| first_paragraph(section_text(&item.body, item.body_line, section)))
        .or_else(|| first_paragraph(&item.body))
        .map(|text| cut(&text))
        .filter(|text| !text.is_empty())
}

fn minutes_of(item: &Item) -> u32 {
    let words: usize = lines(&item.body, item.body_line)
        .into_iter()
        .filter(|(_, _, in_fence)| !in_fence)
        .map(|(_, text, _)| text.split_whitespace().count())
        .sum();
    words.div_ceil(WORDS_PER_MINUTE).max(1) as u32
}

/// The day of each store file's newest commit.
fn commit_days(conn: &Connection) -> Result<HashMap<String, String>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT f.path, max(substr(c.date, 1, 10)) FROM commits c \
         JOIN commit_files f ON f.repo = c.repo AND f.sha = c.sha WHERE f.path LIKE 'stone/%' GROUP BY f.path",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The files of `wanted` that have changes nothing has committed yet, or sit in no repo at all, so
/// only their time on disk tells when they changed. A clean file's time says nothing: a fresh
/// clone touches every file.
pub(crate) fn uncommitted<'a>(repo: &Repo, wanted: impl Iterator<Item = &'a str>) -> HashSet<&'a str> {
    let repos = repo.repos();
    let mut outside = HashSet::new();
    let mut inside: Vec<(&'a str, String)> = Vec::new();
    for path in wanted {
        match repos.repo_of(path) {
            Some((one, _)) => inside.push((path, one.dir().to_string())),
            None => {
                outside.insert(path);
            }
        }
    }
    let dirs: HashSet<&str> = inside.iter().map(|(_, dir)| dir.as_str()).collect();
    let mut dirty = HashSet::new();
    for one in repos.all().iter().filter(|one| dirs.contains(one.dir())) {
        if let Ok(changes) = one.git().working_changes(&[]) {
            dirty.extend(changes.into_iter().map(|change| repos.to_root(one, &change.path)));
        }
    }
    outside.extend(
        inside
            .iter()
            .map(|(path, _)| *path)
            .filter(|path| dirty.contains(*path)),
    );
    outside
}

/// The last change of every store file, by path: its newest commit, or a later edit on disk that
/// nothing has committed yet.
pub(crate) fn updated_days(repo: &Repo) -> Result<BTreeMap<String, String>, EngineError> {
    let commits = match index_with_history(repo)? {
        Some(index) => commit_days(index.conn())?,
        None => HashMap::new(),
    };
    let files = repo.store.stat_files()?;
    let live = uncommitted(repo, files.iter().map(|file| file.path.as_str()));
    let mut out = BTreeMap::new();
    for file in &files {
        let edited = Date::from_unix(file.mtime_ns.div_euclid(1_000_000_000)).iso();
        let day = match commits.get(&file.path) {
            Some(day) if *day >= edited || !live.contains(file.path.as_str()) => day.clone(),
            _ => edited,
        };
        out.insert(file.path.clone(), day);
    }
    Ok(out)
}

pub(crate) fn is_template(id: &str, title: &str) -> bool {
    family(id) == "template" || title.trim_start().starts_with('<')
}

fn row(snapshot: &Snapshot, entry: &Entry, item: &Item, updated: &BTreeMap<String, String>) -> DocRow {
    let id = entry.id.as_str();
    let title = item.fields.title.clone().unwrap_or_else(|| snapshot.label(entry));
    DocRow {
        id: id.to_string(),
        group: family(id).to_lowercase(),
        kind: kind_of(item),
        summary: summary_of(item),
        updated: updated.get(&entry.path).cloned(),
        minutes: minutes_of(item),
        status: item.fields.status.clone(),
        template: is_template(id, &title),
        title,
    }
}

/// Every doc of the store, the most recently changed first.
pub fn docs(repo: &Repo) -> Result<Vec<DocRow>, EngineError> {
    let snapshot = repo.snapshot()?;
    let updated = updated_days(repo)?;
    let mut rows: Vec<DocRow> = snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let item = entry.item.as_ref()?;
            (item.item_type() == Some(ItemType::Doc)).then(|| row(&snapshot, entry, item, &updated))
        })
        .collect();
    let mut sizes: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        *sizes.entry(row.group.clone()).or_default() += 1;
    }
    for row in &mut rows {
        if sizes.get(&row.group).is_none_or(|size| *size < MIN_GROUP) {
            row.group = "other".to_string();
        }
    }
    rows.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.title.cmp(&b.title)));
    Ok(rows)
}
