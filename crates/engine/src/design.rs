//! `stone arch hld|lld`: the two levels of the design (ADR-13) as views of what the store holds.
//! The high-level design lists the systems and containers and how they connect; the low-level
//! design of one container is the service view of `stone code explain` plus the docs written at
//! that level.

use std::collections::{BTreeMap, BTreeSet};

use carvestone_core::{ItemType, code_span_ranges, diagrams, lines};
use serde::Serialize;

use crate::arch_graph::{target_id, use_descriptions};
use crate::check::Problem;
use crate::code::CodeQuery;
use crate::code_entries::{EntryState, entry_state};
use crate::code_graph::{Explain, explain};
use crate::docs_list::{is_lld, kind_of, summary_of, updated_days};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use carvestone_store::Entry;

/// The level a doc names in its frontmatter for the high-level design.
const HLD: &str = "hld";
const LLD: &str = "lld";

/// A system or a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Part {
    pub id: String,
    pub title: String,
    /// `system` or `container`.
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub technology: Option<String>,
    /// The system or container this one sits in.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parent: Option<String>,
    /// `person` or `external` on a system, `store` on a container.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<String>,
}

/// A `uses` entry between two parts, with the phrase the `## Uses` section gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Connection {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
}

/// One `Rel` line of a C4 Context or Container diagram, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DrawnRel {
    /// The item that holds the diagram.
    pub item: String,
    pub line: String,
}

/// A doc marked with a level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DesignDoc {
    pub id: String,
    pub title: String,
    /// The `path#Symbol` the doc names as where the flow starts, when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub start: Option<String>,
    /// What the doc calls itself (`flow`, `component`), when it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<String>,
    /// What it says in a sentence or two.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub summary: Option<String>,
    /// `YYYY-MM-DD` of its last change, where the answer read it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub updated: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Hld {
    pub systems: Vec<Part>,
    pub containers: Vec<Part>,
    pub connections: Vec<Connection>,
    pub drawn: Vec<DrawnRel>,
    pub docs: Vec<DesignDoc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Lld {
    pub container: Part,
    /// The service view; absent when the container has no `code` list yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub service: Option<Explain>,
    pub docs: Vec<DesignDoc>,
}

fn part(snapshot: &Snapshot, entry: &Entry) -> Option<Part> {
    let item = entry.item.as_ref()?;
    let type_name = match item.item_type()? {
        ItemType::System => "system",
        ItemType::Container => "container",
        _ => return None,
    };
    Some(Part {
        id: snapshot.label(entry),
        title: item.fields.title.clone().unwrap_or_else(|| snapshot.label(entry)),
        type_name: type_name.to_string(),
        description: item.fields.description.clone().filter(|d| !d.trim().is_empty()),
        technology: item.fields.technology.clone().filter(|t| !t.trim().is_empty()),
        parent: item.fields.parent.as_deref().map(|p| {
            snapshot
                .resolve(p)
                .map_or_else(|_| p.to_string(), |(e, _)| snapshot.label(e))
        }),
        kind: item.fields.kind.clone().filter(|k| !k.trim().is_empty()),
    })
}

/// The start point a doc names: its first `entry` value, else the first `path#Symbol` in a code
/// span of the body (fenced text is skipped).
fn start_point(item: &carvestone_core::Item) -> Option<String> {
    if let Some(first) = item.fields.entry.iter().find(|e| !e.trim().is_empty()) {
        return Some(first.trim().to_string());
    }
    body_targets(item).into_iter().next().map(|(_, target)| target)
}

/// The text of a code span when it reads `path#Symbol`: a path with an extension, a symbol, no spaces.
pub(crate) fn span_target(span: &str) -> Option<String> {
    let span = span.trim_matches('`').trim();
    let (path, symbol) = span.split_once('#')?;
    (!symbol.is_empty() && path.contains('.') && !span.contains(char::is_whitespace)).then(|| span.to_string())
}

/// Every `path#Symbol` code span of the body outside fences, with its file line.
fn body_targets(item: &carvestone_core::Item) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (line, text, in_fence) in lines(&item.body, item.body_line) {
        if in_fence {
            continue;
        }
        for (start, end) in code_span_ranges(text) {
            out.extend(span_target(&text[start..end]).map(|target| (line, target)));
        }
    }
    out
}

/// Every `path#Symbol` code span in the text of the item's C4 diagrams, with its file line.
fn diagram_targets(item: &carvestone_core::Item) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for diagram in diagrams(&item.body, item.body_line) {
        for (offset, text) in diagram.source.lines().enumerate() {
            for (start, end) in code_span_ranges(text) {
                // The first source line is the one after the opening fence.
                out.extend(span_target(&text[start..end]).map(|target| (diagram.line + 1 + offset, target)));
            }
        }
    }
    out
}

/// `flow` and `diagram-code` warnings: a file or symbol that a doc marked `lld` names as a start
/// point, or a C4 diagram box names, which the code graph no longer holds. Skipped when the code
/// index holds no files.
pub(crate) fn flow_problems(
    snapshot: &Snapshot,
    docs: &[Entry],
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    out: &mut Vec<Problem>,
) -> Result<(), EngineError> {
    if !crate::code_entries::has_code(conn)? {
        return Ok(());
    }
    for entry in snapshot.entries.iter().chain(docs) {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        let mut named: Vec<(&'static str, usize, String)> = Vec::new();
        if item.fields.level.as_deref() == Some(LLD) {
            let line = doc.key_line("entry").unwrap_or(1);
            named.extend(item.fields.entry.iter().map(|t| ("flow", line, t.trim().to_string())));
            named.extend(body_targets(item).into_iter().map(|(line, t)| ("flow", line, t)));
        }
        named.extend(
            diagram_targets(item)
                .into_iter()
                .map(|(line, t)| ("diagram-code", line, t)),
        );
        for (rule, line, target) in named {
            if target.is_empty() {
                continue;
            }
            let what = match entry_state(conn, &target)? {
                EntryState::Found => continue,
                // A file that exists but is not code (a manifest, a doc) is not a start point to judge.
                EntryState::NoFile if root.join(target.split('#').next().unwrap_or("").trim()).is_file() => continue,
                EntryState::NoFile => "a file the code graph does not hold; fix the path or remove it",
                EntryState::NoSymbol => "a symbol that is not in that file; update the name",
            };
            let place = if rule == "flow" { "doc" } else { "diagram in" };
            out.push(Problem::warning(
                &entry.path,
                line,
                rule,
                format!("{place} `{}` names `{target}`, {what}", entry.path),
            ));
        }
    }
    Ok(())
}

/// The docs written at `level`, each with what a reader needs to choose one.
fn marked(
    snapshot: &Snapshot,
    level: &str,
    only: Option<&BTreeSet<String>>,
    updated: &BTreeMap<String, String>,
) -> Vec<DesignDoc> {
    snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let item = entry.item.as_ref()?;
            let at_level = if level == LLD {
                item.item_type() == Some(ItemType::Doc) && is_lld(entry.id.as_str(), item)
            } else {
                item.fields.level.as_deref() == Some(level)
            };
            if !at_level || only.is_some_and(|ids| !ids.contains(entry.id.as_str())) {
                return None;
            }
            Some(DesignDoc {
                id: snapshot.label(entry),
                title: item.fields.title.clone().unwrap_or_else(|| snapshot.label(entry)),
                start: start_point(item),
                kind: kind_of(item),
                summary: summary_of(item),
                updated: updated.get(&entry.path).cloned(),
            })
        })
        .collect()
}

/// The high-level design: systems, containers, how they connect and the docs marked `hld`.
pub fn hld(repo: &Repo) -> Result<Hld, EngineError> {
    let snapshot = repo.snapshot()?;
    let (mut systems, mut containers) = (Vec::new(), Vec::new());
    let mut connections = Vec::new();
    let mut drawn = Vec::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        for diagram in diagrams(&item.body, item.body_line) {
            let first = diagram.source.lines().map(str::trim).find(|l| !l.is_empty());
            if !matches!(first, Some(f) if f.starts_with("C4Context") || f.starts_with("C4Container")) {
                continue;
            }
            for line in diagram.source.lines().map(str::trim) {
                if line.starts_with("Rel") {
                    drawn.push(DrawnRel {
                        item: snapshot.label(entry),
                        line: line.to_string(),
                    });
                }
            }
        }
        let Some(found) = part(&snapshot, entry) else { continue };
        let from = found.id.clone();
        let descriptions = use_descriptions(&snapshot, item);
        for value in &item.fields.uses {
            let to_id = target_id(&snapshot, value);
            let Ok((to_entry, _)) = snapshot.resolve(&to_id) else {
                continue;
            };
            if part(&snapshot, to_entry).is_none() {
                continue;
            }
            connections.push(Connection {
                from: from.clone(),
                to: snapshot.label(to_entry),
                description: descriptions.get(&to_id).cloned(),
            });
        }
        if found.type_name == "system" {
            systems.push(found);
        } else {
            containers.push(found);
        }
    }
    Ok(Hld {
        systems,
        containers,
        connections,
        drawn,
        docs: marked(&snapshot, HLD, None, &BTreeMap::new()),
    })
}

/// The low-level design of one container: its service view and the docs marked `lld` that link it.
pub fn lld(repo: &Repo, target: &str) -> Result<Lld, EngineError> {
    lld_with(repo, target, true)
}

/// `lld` without the service view, for a page that asks `explain` for that on its own.
pub fn lld_docs(repo: &Repo, target: &str) -> Result<Lld, EngineError> {
    lld_with(repo, target, false)
}

fn lld_with(repo: &Repo, target: &str, with_service: bool) -> Result<Lld, EngineError> {
    let snapshot = repo.snapshot()?;
    let containers: Vec<&Entry> = snapshot
        .entries
        .iter()
        .filter(|e| e.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Container))
        .collect();
    let chosen = snapshot
        .resolve(target)
        .ok()
        .map(|(entry, _)| entry)
        .filter(|entry| containers.iter().any(|c| c.id == entry.id));
    let Some(entry) = chosen else {
        let names: Vec<String> = containers.iter().map(|c| snapshot.label(c)).collect();
        return Err(EngineError::Invalid(if names.is_empty() {
            format!(
                "`{target}` is not a container, and this store has no containers; add one with `stone new container`"
            )
        } else {
            format!(
                "`{target}` is not a container; the containers are: {}",
                names.join(", ")
            )
        }));
    };
    let Some(item) = entry.item.as_ref() else {
        return Err(EngineError::Invalid(format!("`{target}` cannot be read")));
    };
    let container = part(&snapshot, entry).expect("a container is a part");

    crate::refresh::refresh(repo)?;
    let mut linking: BTreeSet<String> = repo
        .backlinks(entry.id.as_str(), None)?
        .into_iter()
        // A link from one section of a doc still links the doc.
        .map(|b| b.id.split('#').next().unwrap_or(&b.id).to_string())
        .collect();
    for other in &snapshot.entries {
        let Some(other_item) = &other.item else { continue };
        let names_it = other_item
            .fields
            .components
            .iter()
            .any(|c| snapshot.resolve(c).is_ok_and(|(found, _)| found.id == entry.id));
        if names_it {
            linking.insert(other.id.as_str().to_string());
        }
    }
    let service = if item.fields.code.is_empty() || !with_service {
        None
    } else {
        let query = CodeQuery {
            target: entry.id.as_str().to_string(),
            limit: 20,
            ..CodeQuery::default()
        };
        Some(explain(repo, &query)?)
    };
    Ok(Lld {
        container,
        service,
        docs: marked(&snapshot, LLD, Some(&linking), &updated_days(repo)?),
    })
}
