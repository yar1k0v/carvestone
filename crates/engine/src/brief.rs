//! `stone brief`: a task, its links, entry points, outlines and precedents in one answer.
//!
//! The engine builds the parts; the adapters cut them with [`crate::tiers::fit`]. The task and
//! its links are critical, everything else shrinks to fit.

use std::collections::{HashMap, HashSet};

use carvestone_core::{Section, section_text, sections};
use carvestone_store::Entry;
use serde::Serialize;

use crate::arch_graph::is_arch;
use crate::code::{CodeQuery, OutlineEntry, open, outline};
use crate::code_entries::{EntryState, entries, entry_state};
use crate::design::span_target;
use crate::links::{LinkContext, links_of};
use crate::precedents::{Lookup, is_task, precedents};
use crate::ready::{path_of, plan_lines, plan_spans};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::tiers::{Part, tokens};

/// Tokens a brief aims for when --budget is absent.
pub const DEFAULT_BUDGET: usize = 2000;
/// Entry rows listed per component.
const ENTRIES_PER_COMPONENT: usize = 10;
/// Precedent lines listed.
const PRECEDENTS_SHOWN: usize = 3;
/// The task sections the brief prints whole, as anchor and title.
const TASK_SECTIONS: [(&str, &str); 3] = [
    ("description", "Description"),
    ("acceptance-criteria", "Acceptance criteria"),
    ("plan", "Plan"),
];
/// The relation kinds that make a target a linked item of the task.
const LINK_KINDS: [&str; 3] = ["links", "components", "wikilink"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brief {
    /// The label of the task, such as `CS-12`.
    pub task: String,
    pub parts: Vec<Part>,
}

/// Everything a task needs to start, in print order.
pub fn brief(repo: &Repo, task: &str) -> Result<Brief, EngineError> {
    let snapshot = repo.snapshot()?;
    let (entry, _) = snapshot.resolve(task)?;
    let Some(item) = entry.item.as_ref().filter(|_| is_task(entry)) else {
        return Err(EngineError::Invalid(format!(
            "{} is not a task; give a task id, for example CS-12",
            snapshot.label(entry)
        )));
    };
    let label = snapshot.label(entry);

    let mut parts = vec![Part::critical("task", task_lines(&label, entry))];
    parts.extend(targets_part(repo, item));
    let linked = linked_items(repo, &snapshot, entry);
    if !linked.is_empty() {
        let lines = linked
            .iter()
            .map(|link| match link.target {
                Some(target) => {
                    let body = target.item.as_ref().map_or("", |item| item.body.as_str());
                    let title = target.item.as_ref().and_then(|item| item.fields.title.as_deref());
                    format!(
                        "{}  {}  about {} tokens",
                        snapshot.label(target),
                        title.unwrap_or(""),
                        tokens(body)
                    )
                }
                None => format!("{}  not found; run stone check", link.written),
            })
            .collect();
        let mut part = Part::critical("links", lines);
        part.heading = Some("Links:".to_string());
        parts.push(part);
    }
    for link in &linked {
        if let Some(part) = section_part(&snapshot, link) {
            parts.push(part);
        }
    }

    if !item.fields.components.is_empty() {
        let lines = item
            .fields
            .components
            .iter()
            .flat_map(|id| entry_lines(repo, id))
            .collect();
        parts.push(Part::optional("entry points", Some("Entry points:"), lines));
    }
    let index = (!item.fields.paths.is_empty()).then(|| open(repo).ok()).flatten();
    for path in &item.fields.paths {
        // A file that exists but is not code has nothing to outline.
        let not_code = repo.store.layout.root.join(path).is_file()
            && index
                .as_ref()
                .is_some_and(|index| matches!(entry_state(index.conn(), path), Ok(EntryState::NoFile)));
        if !not_code {
            parts.push(outline_part(repo, path));
        }
    }
    if let Some(part) = precedents_part(repo, &label) {
        parts.push(part);
    }
    Ok(Brief { task: label, parts })
}

/// One item the task links to, with the anchor the link names.
struct Linked<'a> {
    /// The target as the link wrote it.
    written: String,
    anchor: Option<String>,
    target: Option<&'a Entry>,
}

/// The items the task links, in file order, one per target; the task itself is skipped.
fn linked_items<'a>(repo: &Repo, snapshot: &'a Snapshot, entry: &Entry) -> Vec<Linked<'a>> {
    let context = LinkContext::new(&repo.store.config);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in links_of(&context, entry, &[]) {
        if !LINK_KINDS.contains(&row.kind.as_str()) {
            continue;
        }
        let target = snapshot.resolve(&row.dst_item).ok().map(|(found, _)| found);
        if target.is_some_and(|found| found.id == entry.id) {
            continue;
        }
        let key = target.map_or_else(|| row.dst_item.clone(), |found| found.id.as_str().to_string());
        if seen.insert(key) {
            out.push(Linked {
                written: row.dst_item,
                anchor: row.dst_anchor,
                target,
            });
        }
    }
    out
}

/// The label line, then the three task sections whole, the paths and the dependencies.
fn task_lines(label: &str, entry: &Entry) -> Vec<String> {
    let Some(item) = &entry.item else {
        return vec![label.to_string()];
    };
    let mut lines = vec![format!(
        "{label}  {}  {}",
        item.fields.title.as_deref().unwrap_or(""),
        item.fields.status.as_deref().unwrap_or("")
    )];
    let found = sections(&item.body, item.body_line);
    for (at, (anchor, title)) in TASK_SECTIONS.iter().enumerate() {
        if at > 0 {
            lines.push(String::new());
        }
        match found.iter().find(|section| section.anchor == *anchor) {
            Some(section) => lines.extend(text_lines(section_text(&item.body, item.body_line, section))),
            None => lines.push(format!("(no ## {title} section)")),
        }
    }
    if !item.fields.paths.is_empty() {
        lines.push(format!("Paths: {}", item.fields.paths.join(", ")));
    }
    if !item.fields.depends_on.is_empty() {
        lines.push(format!("Depends on: {}", item.fields.depends_on.join(", ")));
    }
    lines
}

/// The code targets of the plan, read as the ready check reads them.
fn targets_part(repo: &Repo, item: &carvestone_core::Item) -> Option<Part> {
    let spans = plan_spans(&plan_lines(item));
    let marked: HashSet<&str> = spans.iter().filter(|s| s.marked).map(|s| s.text.as_str()).collect();
    let mut seen = HashSet::new();
    // One outline per file; `None` is a file whose outline failed.
    let mut outlines: HashMap<String, Option<Vec<OutlineEntry>>> = HashMap::new();
    let mut lines = Vec::new();
    for target in spans.iter().filter_map(|s| span_target(&s.text)) {
        if !seen.insert(target.clone()) {
            continue;
        }
        if marked.contains(target.as_str()) || marked.contains(path_of(&target)) {
            lines.push(format!("{target}  (new)"));
            continue;
        }
        let path = path_of(&target).to_string();
        let found = outlines.entry(path.clone()).or_insert_with(|| {
            let query = CodeQuery {
                target: path.clone(),
                ..CodeQuery::default()
            };
            outline(repo, &query).ok().map(|found| found.entries)
        });
        let Some(entries) = found else {
            // A file on disk that the code graph does not hold (a doc, a manifest) is not judged,
            // as the ready check does not judge it.
            if !repo.store.layout.root.join(&path).is_file() {
                lines.push(format!("{target}  not found"));
            }
            continue;
        };
        let symbol = target.split_once('#').map_or("", |(_, symbol)| symbol);
        // A bare name must not pick the first of several entries; a `Type::method` target is
        // compared with the qualified name, which holds the type with the method.
        let mut matches: Vec<&OutlineEntry> = if symbol.contains("::") {
            let tail = format!("::{symbol}");
            entries
                .iter()
                .filter(|entry| entry.qname == symbol || entry.qname.ends_with(&tail))
                .collect()
        } else {
            entries.iter().filter(|entry| entry.name == symbol).collect()
        };
        matches.sort_by_key(|entry| entry.lines[0]);
        lines.push(match matches.as_slice() {
            [] => format!("{target}  not found"),
            [only] => format!("{target}  {}", range(only)),
            many => {
                let ranges: Vec<String> = many.iter().map(|entry| range(entry)).collect();
                format!("{target}  ambiguous, {} matches: {}", many.len(), ranges.join(", "))
            }
        });
    }
    if lines.is_empty() {
        return None;
    }
    let mut part = Part::critical("plan targets", lines);
    part.heading = Some("Plan targets:".to_string());
    Some(part)
}

fn range(entry: &OutlineEntry) -> String {
    if entry.lines[0] == entry.lines[1] {
        format!("L{}", entry.lines[0])
    } else {
        format!("L{}-{}", entry.lines[0], entry.lines[1])
    }
}

fn text_lines(text: &str) -> Vec<String> {
    text.trim_end().lines().map(str::to_string).collect()
}

/// The text of the section a link names, or the `decision` section when it names none.
/// Architecture items give none: their code comes through the entry points.
fn section_part(snapshot: &Snapshot, link: &Linked) -> Option<Part> {
    let target = link.target?;
    let item = target.item.as_ref()?;
    if is_arch(item.item_type()) {
        return None;
    }
    let section: Section = match &link.anchor {
        Some(anchor) => snapshot.find_section(target, anchor).ok()?,
        None => snapshot.find_section(target, "decision").ok()?,
    };
    let name = format!("{}#{}", snapshot.label(target), section.anchor);
    let lines = text_lines(section_text(&item.body, item.body_line, &section)).split_off(1);
    Some(Part::optional(&name, Some(&format!("{name}:")), lines))
}

/// The entry rows of one component; a failed lookup becomes a line, never an error.
fn entry_lines(repo: &Repo, id: &str) -> Vec<String> {
    let query = CodeQuery {
        target: id.to_string(),
        limit: ENTRIES_PER_COMPONENT,
        ..CodeQuery::default()
    };
    match entries(repo, &query) {
        Ok(found) => {
            let rows: Vec<String> = found
                .groups
                .iter()
                .flat_map(|group| group.entries.iter().map(|row| row.line()))
                .collect();
            if rows.is_empty() {
                vec![format!("{id}: none found")]
            } else {
                rows
            }
        }
        Err(error) => vec![format!("{id}: not available: {}", one_line(&error))],
    }
}

fn outline_part(repo: &Repo, path: &str) -> Part {
    let name = format!("outline {path}");
    let query = CodeQuery {
        target: path.to_string(),
        ..CodeQuery::default()
    };
    match outline(repo, &query) {
        Ok(found) if found.entries.is_empty() => Part::optional(&name, None, vec![format!("{path}: declares nothing")]),
        Ok(found) => Part::optional(
            &name,
            Some(path),
            found.entries.iter().map(|entry| entry.line()).collect(),
        ),
        Err(error) => Part::optional(&name, None, vec![format!("{path}: no outline: {}", one_line(&error))]),
    }
}

fn precedents_part(repo: &Repo, label: &str) -> Option<Part> {
    let lookup = Lookup {
        task: Some(label.to_string()),
        limit: Some(PRECEDENTS_SHOWN),
        ..Lookup::default()
    };
    let lines = match precedents(repo, &lookup) {
        Ok(found) if found.hits.is_empty() => return None,
        Ok(found) => found.hits.iter().map(|hit| hit.line()).collect(),
        Err(error) => vec![format!("not available: {}", one_line(&error))],
    };
    Some(Part::optional("precedents", Some("Precedents:"), lines))
}

/// An error on one line, so a cut by lines never splits it.
fn one_line(error: &EngineError) -> String {
    error.to_string().split_whitespace().collect::<Vec<_>>().join(" ")
}
