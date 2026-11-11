//! What the design lacks: containers with no code, code with no owner, parts with no
//! connection or doc, connections with no phrase, and written build-time links the code contradicts.

use std::collections::BTreeSet;

use carvestone_core::ItemType;
use serde::Serialize;

use crate::arch_graph::{ArchNode, DriftKind, arch_graph};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::scan::{ScanOpts, ScanState, scan};

/// The kinds of gap, in the order the list prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    NoCode,
    UnownedCode,
    NoConnection,
    NoPhrase,
    NoDoc,
    ContradictedLink,
}

impl GapKind {
    /// The name the text list and the JSON both print.
    pub fn as_str(self) -> &'static str {
        match self {
            GapKind::NoCode => "no_code",
            GapKind::UnownedCode => "unowned_code",
            GapKind::NoConnection => "no_connection",
            GapKind::NoPhrase => "no_phrase",
            GapKind::NoDoc => "no_doc",
            GapKind::ContradictedLink => "contradicted_link",
        }
    }
}

/// One thing the design lacks, with the command that works on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gap {
    pub kind: GapKind,
    pub id: String,
    pub detail: String,
    pub next: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gaps {
    pub items: Vec<Gap>,
}

fn gap(kind: GapKind, id: &str, detail: String, next: String) -> Gap {
    Gap {
        kind,
        id: id.to_string(),
        detail,
        next,
    }
}

/// A data store holds no code of its own and needs no doc.
fn is_store(node: &ArchNode) -> bool {
    node.kind.as_deref() == Some("store")
}

/// The ids of the containers some doc links, in its text or in its `components` field.
fn documented(repo: &Repo, snapshot: &Snapshot) -> Result<BTreeSet<String>, EngineError> {
    crate::refresh::refresh(repo)?;
    let is_doc = |id: &str| {
        snapshot
            .entries
            .iter()
            .any(|e| e.id.as_str() == id && e.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Doc))
    };
    let mut out = BTreeSet::new();
    for entry in &snapshot.entries {
        let id = entry.id.as_str();
        for link in repo.backlinks(id, None)? {
            // A link from one section of a doc still links the doc.
            if is_doc(link.id.split('#').next().unwrap_or(&link.id)) {
                out.insert(id.to_string());
            }
        }
    }
    for doc in &snapshot.entries {
        let Some(item) = &doc.item else { continue };
        if item.item_type() != Some(ItemType::Doc) {
            continue;
        }
        for name in &item.fields.components {
            if let Ok((found, _)) = snapshot.resolve(name) {
                out.insert(found.id.as_str().to_string());
            }
        }
    }
    Ok(out)
}

/// Every gap of the design, sorted by kind and then id. It writes nothing: a run-time connection
/// is never a gap, because it has no code edge to check.
pub fn gaps(repo: &Repo) -> Result<Gaps, EngineError> {
    let graph = arch_graph(repo)?;
    let snapshot = repo.snapshot()?;
    let mut items = Vec::new();

    for node in graph
        .nodes
        .iter()
        .filter(|n| n.type_name == "container" || n.type_name == "component")
    {
        if node.code.is_empty() && !is_store(node) {
            items.push(gap(
                GapKind::NoCode,
                &node.id,
                format!("{} {} lists no code", node.type_name, node.id),
                "stone map --write".into(),
            ));
        }
    }

    let dry = ScanOpts {
        dry_run: true,
        ..ScanOpts::default()
    };
    for (path, state) in scan(repo, &dry)?.entries {
        if matches!(state, ScanState::New | ScanState::Changed) {
            items.push(gap(
                GapKind::UnownedCode,
                &path,
                "code that no part of the design owns".into(),
                "stone scan".into(),
            ));
        }
    }

    // A diagram labels every line, so a connection between two parts needs a phrase.
    let known: BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for edge in graph
        .uses
        .iter()
        .filter(|u| u.description.is_none() && known.contains(u.from.as_str()) && known.contains(u.to.as_str()))
    {
        // Following the command must keep the protocol the bullet already names.
        // Double quotes work in bash and in PowerShell; a value with a space needs them.
        let over = edge.technology.as_ref().map_or(String::new(), |t| {
            if t.contains(char::is_whitespace) {
                format!(" --over \"{t}\"")
            } else {
                format!(" --over {t}")
            }
        });
        items.push(gap(
            GapKind::NoPhrase,
            &edge.from,
            format!("{} uses {} and says nothing about what it does", edge.from, edge.to),
            format!("stone arch connect {} {} --does <text>{over}", edge.from, edge.to),
        ));
    }

    let documented = documented(repo, &snapshot)?;
    for node in graph.nodes.iter().filter(|n| n.type_name == "container") {
        if !graph.uses.iter().any(|u| u.from == node.id || u.to == node.id) {
            items.push(gap(
                GapKind::NoConnection,
                &node.id,
                format!("container {} neither uses nor is used by another part", node.id),
                format!("stone arch connect {} <to> --does <text>", node.id),
            ));
        }
        if !is_store(node) && !documented.contains(&node.id) {
            items.push(gap(
                GapKind::NoDoc,
                &node.id,
                format!("no doc links container {}", node.id),
                format!("stone new doc --title <title> --link {}", node.id),
            ));
        }
    }

    for finding in &graph.drift {
        let next = match (finding.kind, &finding.to) {
            (DriftKind::Code, Some(to)) => format!("stone arch connect {} {to} --does <text>", finding.from),
            (DriftKind::Code, None) => format!("stone arch connect {} <to> --does <text>", finding.from),
            (DriftKind::Uses, _) => format!("stone edit {}", finding.from),
        };
        items.push(gap(
            GapKind::ContradictedLink,
            &finding.from,
            finding.problem.message.clone(),
            next,
        ));
    }

    items.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.id.cmp(&b.id)));
    Ok(Gaps { items })
}
