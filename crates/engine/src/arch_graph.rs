//! Component edges: the code dependencies between arch elements, read from
//! the code edges and the elements' `code` globs, and the drift warnings that
//! compare them with the curated `uses` lists.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use carvestone_core::{Item, ItemType, diagrams, uses_bullets};
use carvestone_graph::ProjectFacts;
use rusqlite::Connection;
use serde::Serialize;

use crate::check::Problem;
use crate::code::open;
use crate::code_rank::is_support_path;
use crate::owner::{Element, arch_elements};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::service_map::projects_in;

/// Code edges from the files one element owns to the files another owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ComponentEdge {
    pub from: String,
    pub to: String,
    pub edges: u32,
    /// The first site in path and line order: (file, line).
    pub first: (String, u32),
    /// No exact edge backs it, only inferred ones.
    pub inferred_only: bool,
}

impl ComponentEdge {
    /// `12 edges, first src/a.cs:30` with `, inferred only` when it applies.
    pub fn summary(&self) -> String {
        let noun = if self.edges == 1 { "edge" } else { "edges" };
        let note = if self.inferred_only { ", inferred only" } else { "" };
        format!("{} {noun}, first {}:{}{note}", self.edges, self.first.0, self.first.1)
    }
}

/// Edges counted between one pair of elements.
struct Tally {
    edges: u32,
    exact: u32,
    first: (String, u32),
}

struct ComponentGraph {
    elements: Vec<Element>,
    edges: Vec<ComponentEdge>,
    /// Elements that own at least one file the code index holds.
    covered: BTreeSet<String>,
}

fn owners<'a>(elements: &'a [Element], path: &'a str) -> impl Iterator<Item = &'a str> {
    elements.iter().filter(move |e| e.owns(path)).map(|e| e.id.as_str())
}

fn build(snapshot: &Snapshot, conn: &Connection) -> Result<ComponentGraph, EngineError> {
    // Glob problems are `stone check`'s to report; here a bad glob simply claims nothing.
    let elements = arch_elements(snapshot, &mut Vec::new());
    let mut covered = BTreeSet::new();
    // A file with no symbols besides its own (a language without an extractor yet) says nothing.
    let mut stmt = conn.prepare(
        "SELECT f.path FROM files f WHERE f.lang IS NOT NULL AND f.lang <> 'project' \
         AND EXISTS (SELECT 1 FROM symbols s WHERE s.file_id = f.id AND s.kind != 'file')",
    )?;
    let paths: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
    for path in &paths {
        covered.extend(owners(&elements, path).map(str::to_string));
    }
    // An edge with an unowned end counts for nobody; one with a null target is not resolved.
    let mut stmt = conn.prepare(
        "SELECT sf.path, df.path, count(*), sum(e.exact), min(e.line) \
         FROM edge_ends e JOIN symbols s ON s.id = e.src_id JOIN files sf ON sf.id = s.file_id \
         JOIN symbols d ON d.id = e.dst_id JOIN files df ON df.id = d.file_id \
         WHERE e.dst_id IS NOT NULL AND s.file_id != d.file_id AND e.kind <> 'dispatch' \
         GROUP BY sf.path, df.path ORDER BY sf.path, df.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u32>(2)?,
            row.get::<_, u32>(3)?,
            row.get::<_, u32>(4)?,
        ))
    })?;
    let mut pairs: BTreeMap<(String, String), Tally> = BTreeMap::new();
    for row in rows {
        let (src, dst, n, exact, line) = row?;
        // Tests, benchmarks and generated code reach everywhere; they are not how the parts depend on each other.
        if is_support_path(&src) || is_support_path(&dst) {
            continue;
        }
        for a in owners(&elements, &src) {
            for b in owners(&elements, &dst).filter(|b| *b != a) {
                let site = (src.clone(), line);
                let tally = pairs.entry((a.to_string(), b.to_string())).or_insert_with(|| Tally {
                    edges: 0,
                    exact: 0,
                    first: site.clone(),
                });
                tally.edges += n;
                tally.exact += exact;
                if site < tally.first {
                    tally.first = site;
                }
            }
        }
    }
    let edges = pairs
        .into_iter()
        .map(|((from, to), tally)| ComponentEdge {
            from,
            to,
            edges: tally.edges,
            first: tally.first,
            inferred_only: tally.exact == 0,
        })
        .collect();
    Ok(ComponentGraph {
        elements,
        edges,
        covered,
    })
}

/// Component edges from the code index, sorted by `from` then `to`.
pub(crate) fn component_edges_in(snapshot: &Snapshot, conn: &Connection) -> Result<Vec<ComponentEdge>, EngineError> {
    Ok(build(snapshot, conn)?.edges)
}

/// Every component edge of the repository, with the index brought up to date first.
pub fn component_edges(repo: &Repo) -> Result<Vec<ComponentEdge>, EngineError> {
    let index = open(repo)?;
    component_edges_in(&repo.snapshot()?, index.conn())
}

/// An arch item as a node of the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArchNode {
    pub id: String,
    pub short: Option<String>,
    pub path: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub parent: Option<String>,
    /// `person` or `external` on a system, `store` on a container.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub technology: Option<String>,
    /// The globs of code this element claims.
    pub code: Vec<String>,
}

/// A dependency written by hand in an element's `uses` list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UsesEdge {
    pub from: String,
    pub to: String,
    /// The verb phrase the element's `## Uses` section gives this target.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    /// How the link leaves the process, from the `## Uses` bullet's trailing `(technology)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub technology: Option<String>,
}

/// The target an item's `uses` entry or `## Uses` bullet names, as an item id; unresolved text stays as written.
pub(crate) fn target_id(snapshot: &Snapshot, value: &str) -> String {
    snapshot.resolve(value.trim()).map_or_else(
        |_| value.trim().to_string(),
        |(target, _)| target.id.as_str().to_string(),
    )
}

/// What an item's `## Uses` section says about each target, by target id. The first bullet with a
/// phrase wins when a target appears twice.
pub(crate) fn use_descriptions(snapshot: &Snapshot, item: &Item) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for bullet in uses_bullets(&item.body, item.body_line) {
        if !bullet.description.is_empty() {
            out.entry(target_id(snapshot, &bullet.target))
                .or_insert(bullet.description);
        }
    }
    out
}

/// The technology an item's `## Uses` section gives each target, by target id. The first bullet
/// with a technology wins when a target appears twice.
pub(crate) fn use_technologies(snapshot: &Snapshot, item: &Item) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for bullet in uses_bullets(&item.body, item.body_line) {
        if let Some(technology) = bullet.technology {
            out.entry(target_id(snapshot, &bullet.target)).or_insert(technology);
        }
    }
    out
}

/// Which side of a drift finding is wrong: `code` is a dependency the code has and `uses` does not
/// list, `uses` is a `uses` entry no code edge backs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum DriftKind {
    Code,
    Uses,
}

/// One `arch-drift` finding with the element ids it names, so the map draws it without reading the
/// sentence. `to` is absent for a kind that names one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DriftFinding {
    #[serde(flatten)]
    pub problem: Problem,
    pub kind: DriftKind,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub to: Option<String>,
}

/// Everything the architecture map draws: the elements, what they say they use, what the code
/// says they use, and where the two disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArchGraph {
    pub nodes: Vec<ArchNode>,
    pub uses: Vec<UsesEdge>,
    pub code_edges: Vec<ComponentEdge>,
    pub drift: Vec<DriftFinding>,
    pub diagrams: Vec<DiagramRef>,
}

/// One C4 diagram written in a store item, in item order and then body order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiagramRef {
    /// The item that holds it.
    pub item: String,
    /// The diagram's own title, else the item's title, else the item id.
    pub title: String,
    /// File line of the opening fence.
    pub line: u32,
    /// The Mermaid C4 text inside the fence.
    pub source: String,
}

/// The arch items as nodes and their `uses` entries as edges, in entry order. Reads no code index.
pub(crate) fn arch_nodes(snapshot: &Snapshot) -> (Vec<ArchNode>, Vec<UsesEdge>) {
    let mut nodes = Vec::new();
    let mut uses = Vec::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        if !is_arch(item.item_type()) {
            continue;
        }
        let id = entry.id.as_str().to_string();
        let descriptions = use_descriptions(snapshot, item);
        let technologies = use_technologies(snapshot, item);
        for value in &item.fields.uses {
            let to = target_id(snapshot, value);
            let description = descriptions.get(&to).cloned();
            let technology = technologies.get(&to).cloned();
            uses.push(UsesEdge {
                from: id.clone(),
                to,
                description,
                technology,
            });
        }
        nodes.push(ArchNode {
            short: snapshot.scheme.short(&entry.id),
            path: entry.path.clone(),
            type_name: item.type_name.clone().unwrap_or_default(),
            title: item.fields.title.clone(),
            status: item.fields.status.clone(),
            parent: item.fields.parent.clone(),
            kind: item.fields.kind.clone().filter(|k| !k.trim().is_empty()),
            description: item.fields.description.clone().filter(|d| !d.trim().is_empty()),
            technology: item.fields.technology.clone().filter(|t| !t.trim().is_empty()),
            code: item.fields.code.clone(),
            id,
        });
    }
    (nodes, uses)
}

/// The architecture map of the repository, with the index brought up to date first.
pub fn arch_graph(repo: &Repo) -> Result<ArchGraph, EngineError> {
    let index = open(repo)?;
    let snapshot = repo.snapshot()?;
    let graph = build(&snapshot, index.conn())?;
    let drift = drift_of(&graph, &repo.store.layout.root, &snapshot, index.conn())?;
    let code_edges = graph.edges;
    let mut found = Vec::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        for diagram in diagrams(&item.body, item.body_line) {
            found.push(DiagramRef {
                item: entry.id.as_str().to_string(),
                title: diagram
                    .title
                    .or_else(|| item.fields.title.clone())
                    .unwrap_or_else(|| entry.id.as_str().to_string()),
                line: diagram.line as u32,
                source: diagram.source,
            });
        }
    }
    let (nodes, uses) = arch_nodes(&snapshot);
    Ok(ArchGraph {
        nodes,
        uses,
        code_edges,
        drift,
        diagrams: found,
    })
}

/// `arch-drift` warnings: code dependencies with at least one exact edge `uses` does not list,
/// and `uses` entries no code edge backs.
/// Skipped when the code index holds no files.
pub(crate) fn drift_problems(
    root: &Path,
    snapshot: &Snapshot,
    conn: &Connection,
    out: &mut Vec<Problem>,
) -> Result<(), EngineError> {
    out.extend(
        drift_of(&build(snapshot, conn)?, root, snapshot, conn)?
            .into_iter()
            .map(|finding| finding.problem),
    );
    Ok(())
}

/// The element pairs a project file already declares: the project one element owns references,
/// directly or through other projects, a project another element owns.
fn declared(elements: &[Element], projects: &[ProjectFacts]) -> BTreeSet<(String, String)> {
    let by_path: BTreeMap<&str, usize> = projects.iter().enumerate().map(|(i, p)| (p.path.as_str(), i)).collect();
    let mut out = BTreeSet::new();
    for (start, project) in projects.iter().enumerate() {
        let from: Vec<&str> = owners(elements, &project.path).collect();
        if from.is_empty() {
            continue;
        }
        let mut queue = vec![start];
        let mut seen = BTreeSet::from([start]);
        while let Some(at) = queue.pop() {
            for reference in &projects[at].references {
                let Some(&next) = by_path.get(reference.path.as_str()) else {
                    continue;
                };
                if !seen.insert(next) {
                    continue;
                }
                queue.push(next);
                for to in owners(elements, &projects[next].path) {
                    out.extend(from.iter().map(|from| (from.to_string(), to.to_string())));
                }
            }
        }
    }
    out
}

/// The same findings as `drift_problems`, with the ids each one names.
fn drift_of(
    graph: &ComponentGraph,
    root: &Path,
    snapshot: &Snapshot,
    conn: &Connection,
) -> Result<Vec<DriftFinding>, EngineError> {
    let mut out = Vec::new();
    if graph.covered.is_empty() {
        return Ok(out);
    }
    // A project reference is a dependency written down already; `uses` need not repeat it.
    let declared = declared(&graph.elements, &projects_in(root, conn)?);
    for entry in &snapshot.entries {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        let id = entry.id.as_str();
        if !graph.elements.iter().any(|e| e.id == id) {
            continue;
        }
        let uses: BTreeSet<String> = item
            .fields
            .uses
            .iter()
            .map(|value| target_id(snapshot, value))
            .collect();
        let line = doc.key_line("uses").or_else(|| doc.key_line("code")).unwrap_or(1);
        // A name guess alone is not evidence of a dependency.
        for edge in graph.edges.iter().filter(|e| e.from == id && !e.inferred_only) {
            if !uses.contains(&edge.to) && !declared.contains(&(edge.from.clone(), edge.to.clone())) {
                out.push(DriftFinding {
                    problem: Problem::warning(
                        &entry.path,
                        line,
                        "arch-drift",
                        format!(
                            "{id} depends on {} in code ({}) but does not list it in uses",
                            edge.to,
                            edge.summary()
                        ),
                    ),
                    kind: DriftKind::Code,
                    from: id.to_string(),
                    to: Some(edge.to.clone()),
                });
            }
        }
        if !graph.covered.contains(id) {
            continue;
        }
        // A link that leaves the process (HTTP, a queue, SQL) has no code edge to show.
        let remote: BTreeSet<String> = uses_bullets(&item.body, item.body_line)
            .into_iter()
            .filter(|bullet| bullet.leaves_process())
            .map(|bullet| target_id(snapshot, &bullet.target))
            .collect();
        for target in uses
            .iter()
            .filter(|t| t.as_str() != id && graph.covered.contains(*t) && !remote.contains(*t))
        {
            if !graph.edges.iter().any(|e| e.from == id && e.to == *target) {
                out.push(DriftFinding {
                    problem: Problem::warning(
                        &entry.path,
                        line,
                        "arch-drift",
                        format!("{id} lists {target} in uses but no code edge exists"),
                    ),
                    kind: DriftKind::Uses,
                    from: id.to_string(),
                    to: Some(target.clone()),
                });
            }
        }
    }
    Ok(out)
}

/// Whether an item of this type can have code dependencies.
pub(crate) fn is_arch(item_type: Option<ItemType>) -> bool {
    matches!(
        item_type,
        Some(ItemType::System | ItemType::Container | ItemType::Component)
    )
}
