//! `stone code impact|path|explain`: traversals over the resolved edges
//! (petgraph), joined to components, open tasks and docs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use carvestone_core::{ItemType, uses_bullets};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use petgraph::Direction;
use petgraph::algo::astar;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use rusqlite::Connection;
use serde::Serialize;

use crate::code::{CodeQuery, Target, cut, effective_limit, exactness, is_zero, open, resolve_target, symbol_hit};
use crate::code_entries::{EntryRow, component_rows, detect};
use crate::code_gap;
use crate::code_index::{building_note, contains};
use crate::code_rank::{is_support_path, rank};
use crate::modules::{Module, common_folder, stored as stored_modules};
use crate::owner::{Element, arch_elements};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::search::Hit;
use crate::tiers;
use carvestone_graph::project_facts;

const DEFAULT_DEPTH: usize = 3;
const KEY_SYMBOLS: usize = 10;
/// Statuses after which a task no longer counts as open.
const CLOSED: [&str; 2] = ["done", "archived"];
/// Edge costs for `path`: fewest steps first, an exact edge breaks the tie.
const COST_EXACT: u32 = 10;
const COST_INFERRED: u32 = 11;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Impact {
    pub query: String,
    /// Dependent symbols and files, nearest first; each hit says `depth N` and is marked exact or inferred.
    pub dependents: Vec<Hit>,
    pub components: Vec<String>,
    pub tasks: Vec<String>,
    pub docs: Vec<String>,
    pub truncated: bool,
    /// Dependents the limit or the budget left out.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of what the limit or the budget left out, as one line per dropped row.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Set only on an empty answer: what is indexed, what is not, and the grep command to run next.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PathResult {
    pub from: String,
    pub to: String,
    /// In walked order. Empty when there is no path.
    pub steps: Vec<Hit>,
    /// True only when every step is exact.
    pub exact: bool,
    /// `forward` (from to to), `reverse` (to to from), `undirected`, or `none`.
    pub direction: String,
    pub truncated: bool,
    /// Steps at the end of the chain the limit or the budget left out.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of what the limit or the budget left out, as one line per dropped row.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Set only on an empty answer: what is indexed, what is not, and the grep command to run next.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Dependency {
    /// The component on the other side, or `(unowned)`.
    pub component: String,
    pub exact: u32,
    pub inferred: u32,
}

/// What a component says it does with a component it uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UseLink {
    pub target: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Explain {
    pub target: String,
    /// Where execution starts: the component's declared `entry` list first, then detected entries.
    pub entries: Vec<EntryRow>,
    /// The `## Uses` bullets that carry a phrase.
    pub uses: Vec<UseLink>,
    pub files: Vec<Hit>,
    /// Up to 10 symbols by inbound degree.
    pub key_symbols: Vec<Hit>,
    pub inbound: Vec<Dependency>,
    pub outbound: Vec<Dependency>,
    pub tasks: Vec<String>,
    pub docs: Vec<String>,
    pub truncated: bool,
    /// Files and key symbols the limit or the budget left out.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of what the limit or the budget left out, as one line per dropped row.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// The top folders of a component or folder, with their files: its inside view.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<FolderRow>>", optional))]
    pub folders: Vec<FolderRow>,
    /// Code edges between those folders, most first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<FolderEdge>>", optional))]
    pub folder_edges: Vec<FolderEdge>,
    /// External packages its project files name, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub packages: Vec<String>,
    /// Projects outside it that its project files reference.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Library>>", optional))]
    pub libraries: Vec<Library>,
}

/// One top folder of what `explain` looks at. `folder` is a valid explain target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FolderRow {
    pub folder: String,
    pub files: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FolderEdge {
    pub from: String,
    pub to: String,
    pub exact: u32,
    pub inferred: u32,
}

/// A project referenced from outside: its name, its folder (an explain target) and the element
/// that owns it, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Library {
    pub name: String,
    pub folder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub component: Option<String>,
}

// ---------------------------------------------------------------- graph

#[derive(Debug, Clone)]
struct Edge {
    exact: bool,
    kind: String,
}

struct CodeGraph {
    graph: DiGraph<i64, Edge>,
    index: HashMap<i64, NodeIndex>,
}

impl CodeGraph {
    /// Every resolved edge, with name buckets expanded to their members. An
    /// edge whose target the resolver could not pin has no end to walk to.
    fn load(conn: &Connection, exact_only: bool) -> Result<CodeGraph, EngineError> {
        let mut stmt = conn.prepare(
            "SELECT src_id, dst_id, kind, exact FROM edge_ends \
             WHERE dst_id IS NOT NULL AND (?1 = 0 OR exact = 1) \
             ORDER BY src_id, dst_id, kind, exact DESC, line",
        )?;
        let mut graph = DiGraph::new();
        let mut index: HashMap<i64, NodeIndex> = HashMap::new();
        let rows = stmt.query_map([exact_only], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        for row in rows {
            let (src, dst, kind, exact) = row?;
            let a = *index.entry(src).or_insert_with(|| graph.add_node(src));
            let b = *index.entry(dst).or_insert_with(|| graph.add_node(dst));
            graph.add_edge(a, b, Edge { exact, kind });
        }
        Ok(CodeGraph { graph, index })
    }
}

struct Sym {
    qname: String,
    kind: String,
    signature: String,
    lines: [u32; 2],
    path: String,
}

fn sym(conn: &Connection, id: i64) -> Result<Sym, EngineError> {
    Ok(conn
        .prepare_cached(
            "SELECT s.qname, s.kind, s.signature, s.line_start, s.line_end, f.path \
             FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.id = ?1",
        )?
        .query_row([id], |row| {
            Ok(Sym {
                qname: row.get(0)?,
                kind: row.get(1)?,
                signature: row.get(2)?,
                lines: [row.get(3)?, row.get(4)?],
                path: row.get(5)?,
            })
        })?)
}

fn hit_of(s: &Sym, exact: bool, detail: &str) -> Hit {
    let snippet = format!("{detail} {}", s.signature).trim().to_string();
    symbol_hit(&s.qname, &s.kind, &s.path, s.lines, exactness(exact), snippet)
}

// ---------------------------------------------------------------- components, tasks, docs

fn glob_set(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = pattern.trim();
        // A directory path covers everything under it.
        let pattern = if pattern.ends_with('/') {
            format!("{pattern}**")
        } else {
            pattern.to_string()
        };
        if let Ok(glob) = GlobBuilder::new(&pattern).literal_separator(true).build() {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// The arch elements that own `path`, by their ids.
fn owners_of(elements: &[Element], path: &str) -> BTreeSet<String> {
    elements.iter().filter(|e| e.owns(path)).map(|e| e.id.clone()).collect()
}

fn is_open(status: Option<&str>) -> bool {
    status.is_none_or(|s| !CLOSED.contains(&s))
}

/// Open tasks and docs that cover `owners` (their `components`) or the code itself
/// (a task's `paths`, a doc's `[[code:...]]` links).
fn related(
    snapshot: &Snapshot,
    conn: &Connection,
    owners: &BTreeSet<String>,
    paths: &BTreeSet<String>,
    qnames: &BTreeSet<String>,
) -> Result<(Vec<String>, Vec<String>), EngineError> {
    let mut code_linked: BTreeSet<String> = BTreeSet::new();
    let mut stmt = conn.prepare("SELECT DISTINCT src_item, dst_item FROM links WHERE kind = 'code'")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    for row in rows {
        let (src, dst) = row?;
        let target = dst.strip_prefix("code:").unwrap_or(&dst);
        let range_free = target
            .rsplit_once(':')
            .filter(|(_, tail)| tail.chars().all(|c| c.is_ascii_digit() || c == '-'))
            .map_or(target, |(head, _)| head);
        if qnames.contains(target) || paths.contains(target) || paths.contains(range_free) {
            code_linked.insert(src);
        }
    }

    let mut tasks = BTreeSet::new();
    let mut docs = BTreeSet::new();
    for entry in &snapshot.entries {
        let Some(item) = &entry.item else { continue };
        let covers_component = item.fields.components.iter().any(|value| {
            snapshot
                .resolve(value)
                .is_ok_and(|(target, _)| owners.contains(target.id.as_str()))
        });
        match item.item_type() {
            Some(ItemType::Task) => {
                let covers_path =
                    !item.fields.paths.is_empty() && paths.iter().any(|p| glob_set(&item.fields.paths).is_match(p));
                if is_open(item.fields.status.as_deref()) && (covers_component || covers_path) {
                    tasks.insert(snapshot.label(entry));
                }
            }
            Some(ItemType::Doc | ItemType::Plan | ItemType::Decision | ItemType::Reference) => {
                if covers_component || code_linked.contains(entry.id.as_str()) {
                    docs.insert(snapshot.label(entry));
                }
            }
            _ => {}
        }
    }
    Ok((tasks.into_iter().collect(), docs.into_iter().collect()))
}

fn elements_of(snapshot: &Snapshot) -> Vec<Element> {
    // Glob problems are `stone check`'s to report; here a bad glob simply claims nothing.
    arch_elements(snapshot, &mut Vec::new())
}

// ---------------------------------------------------------------- impact

struct Dependent {
    level: usize,
    exact: bool,
    /// How likely an inferred dependent is real, against the target.
    likely: (bool, usize),
    hit: Hit,
}

/// Everything that depends on the target, transitively over incoming edges.
pub fn impact(repo: &Repo, q: &CodeQuery) -> Result<Impact, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let target = resolve_target(conn, &q.target)?;
    let graph = CodeGraph::load(conn, q.exact_only)?;
    let depth = q.depth.unwrap_or(DEFAULT_DEPTH);

    let mut seen: HashSet<i64> = target.ids.iter().copied().collect();
    let mut frontier: Vec<(i64, bool)> = target.ids.iter().map(|id| (*id, true)).collect();
    let mut found: Vec<(i64, usize, bool)> = Vec::new();
    for level in 1..=depth {
        // Within a level a node is exact when any all-exact route reaches it.
        let mut next: BTreeMap<i64, bool> = BTreeMap::new();
        for (id, through_exact) in &frontier {
            let Some(node) = graph.index.get(id) else { continue };
            for edge in graph.graph.edges_directed(*node, Direction::Incoming) {
                let from = graph.graph[edge.source()];
                if seen.contains(&from) {
                    continue;
                }
                let exact = *through_exact && edge.weight().exact;
                next.entry(from).and_modify(|e| *e |= exact).or_insert(exact);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next.iter().map(|(id, exact)| (*id, *exact)).collect();
        for (id, exact) in next {
            seen.insert(id);
            found.push((id, level, exact));
        }
    }

    let mut dependents: Vec<Dependent> = Vec::with_capacity(found.len());
    let mut paths: BTreeSet<String> = BTreeSet::new();
    let mut qnames: BTreeSet<String> = BTreeSet::new();
    for id in &target.ids {
        let s = sym(conn, *id)?;
        paths.insert(s.path);
        qnames.insert(s.qname);
    }
    let target_paths: Vec<String> = paths.iter().cloned().collect();
    for (id, level, exact) in found {
        let s = sym(conn, id)?;
        paths.insert(s.path.clone());
        qnames.insert(s.qname.clone());
        // An inferred dependent nearer the target, in the same kind of code, comes first.
        let likely = if exact {
            (false, 0)
        } else {
            target_paths.iter().map(|p| rank(&s.path, p)).max().unwrap_or_default()
        };
        dependents.push(Dependent {
            level,
            exact,
            likely,
            hit: hit_of(&s, exact, &format!("depth {level}")),
        });
    }
    dependents.sort_by(|a, b| {
        a.level
            .cmp(&b.level)
            .then_with(|| b.exact.cmp(&a.exact))
            .then_with(|| b.likely.cmp(&a.likely))
            .then_with(|| a.hit.path.cmp(&b.hit.path))
            .then_with(|| a.hit.lines[0].cmp(&b.hit.lines[0]))
            .then_with(|| a.hit.id.cmp(&b.hit.id))
    });

    let snapshot = repo.snapshot()?;
    let elements = elements_of(&snapshot);
    let mut owners = BTreeSet::new();
    for path in &paths {
        owners.extend(owners_of(&elements, path));
    }
    let components: Vec<String> = elements
        .iter()
        .filter(|e| owners.contains(&e.id))
        .map(|e| e.label.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let (tasks, docs) = related(&snapshot, conn, &owners, &paths, &qnames)?;

    let (dependents, omitted, left_out) = cut(dependents.into_iter().map(|d| d.hit).collect(), q.limit, q.budget);
    // Built before the target moves into the answer.
    let gap = if dependents.is_empty() {
        Some(code_gap::of_target(conn, &target)?)
    } else {
        None
    };
    Ok(Impact {
        query: target.label,
        dependents,
        components,
        tasks,
        docs,
        truncated: omitted > 0,
        omitted,
        left_out,
        note: building_note(conn)?,
        gap,
    })
}

// ---------------------------------------------------------------- path

fn cost(edge: &Edge) -> u32 {
    if edge.kind.is_empty() {
        0
    } else if edge.exact {
        COST_EXACT
    } else {
        COST_INFERRED
    }
}

/// The cheapest walk from any of `starts` to any of `goals`, as nodes and the edge taken into each.
fn shortest(graph: &CodeGraph, starts: &[i64], goals: &[i64], undirected: bool) -> Option<Vec<(i64, Option<Edge>)>> {
    let goal_set: HashSet<i64> = goals.iter().copied().collect();
    if let Some(shared) = starts.iter().find(|id| goal_set.contains(id)) {
        return Some(vec![(*shared, None)]);
    }
    let mut work = graph.graph.clone();
    if undirected {
        for edge in graph.graph.edge_references() {
            work.add_edge(edge.target(), edge.source(), edge.weight().clone());
        }
    }
    let source = work.add_node(-1);
    for id in starts {
        if let Some(node) = graph.index.get(id) {
            work.add_edge(
                source,
                *node,
                Edge {
                    exact: true,
                    kind: String::new(),
                },
            );
        }
    }
    let (_, nodes) = astar(
        &work,
        source,
        |node| goal_set.contains(&work[node]),
        |edge| cost(edge.weight()),
        |_| 0,
    )?;
    let mut steps: Vec<(i64, Option<Edge>)> = Vec::new();
    for pair in nodes.windows(2) {
        let best = work
            .edges_connecting(pair[0], pair[1])
            .map(|edge| edge.weight())
            .min_by(|a, b| cost(a).cmp(&cost(b)).then_with(|| a.kind.cmp(&b.kind)));
        let via = (pair[0] != source).then(|| best.cloned()).flatten();
        steps.push((work[pair[1]], via));
    }
    Some(steps)
}

/// The shortest chain of edges between two targets: a to b, else b to a, else ignoring direction.
pub fn path(repo: &Repo, q: &CodeQuery) -> Result<PathResult, EngineError> {
    let Some(to_text) = q.to.as_deref() else {
        return Err(EngineError::Invalid(
            "give two targets: stone code path <a> <b>".to_string(),
        ));
    };
    let index = open(repo)?;
    let conn = index.conn();
    let a = resolve_target(conn, &q.target)?;
    let b = resolve_target(conn, to_text)?;
    let graph = CodeGraph::load(conn, q.exact_only)?;

    let attempts: [(&str, &Target, &Target, bool); 3] = [
        ("forward", &a, &b, false),
        ("reverse", &b, &a, false),
        ("undirected", &a, &b, true),
    ];
    let mut walked = None;
    for (label, start, goal, undirected) in attempts {
        if let Some(steps) = shortest(&graph, &start.ids, &goal.ids, undirected) {
            walked = Some((label, steps));
            break;
        }
    }
    let note = building_note(conn)?;
    let Some((direction, steps)) = walked else {
        let gap = code_gap::of_targets(conn, &a, &b)?;
        return Ok(PathResult {
            from: a.label,
            to: b.label,
            steps: Vec::new(),
            exact: false,
            direction: "none".to_string(),
            truncated: false,
            omitted: 0,
            left_out: None,
            note,
            gap: Some(gap),
        });
    };
    let exact = steps.iter().all(|(_, via)| via.as_ref().is_none_or(|edge| edge.exact));
    let mut hits = Vec::with_capacity(steps.len());
    for (id, via) in &steps {
        let s = sym(conn, *id)?;
        hits.push(match via {
            None => hit_of(&s, true, "start"),
            Some(edge) => hit_of(&s, edge.exact, &edge.kind),
        });
    }
    let (steps, omitted, left_out) = cut(hits, q.limit, q.budget);
    Ok(PathResult {
        from: a.label,
        to: b.label,
        steps,
        exact,
        direction: direction.to_string(),
        truncated: omitted > 0,
        omitted,
        left_out,
        note,
        gap: None,
    })
}

// ---------------------------------------------------------------- explain

fn id_list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// What `explain` looks at: a component by its `code` globs, a module by its member files, or
/// a folder by its path.
enum Scope<'a> {
    Component(&'a Element),
    Module(&'a Module),
    Folder(String),
}

impl Scope<'_> {
    fn contains(&self, path: &str) -> bool {
        match self {
            Scope::Component(element) => element.globs.is_match(path),
            Scope::Module(module) => module.files.binary_search_by(|f| f.as_str().cmp(path)).is_ok(),
            Scope::Folder(folder) => contains(folder, path),
        }
    }
}

/// The top folder of `path` under `root`, or `root` itself for a file directly in it.
fn top_folder(root: &str, path: &str) -> String {
    let rest = if root == "." {
        path
    } else {
        path.strip_prefix(root)
            .and_then(|r| r.strip_prefix('/'))
            .unwrap_or(path)
    };
    match rest.split_once('/') {
        Some((top, _)) if root == "." => top.to_string(),
        Some((top, _)) => format!("{root}/{top}"),
        None => root.to_string(),
    }
}

/// External packages and referenced outside projects of the project files `scope` holds.
fn project_view(
    repo: &Repo,
    conn: &Connection,
    scope: &Scope,
    elements: &[Element],
) -> Result<(Vec<String>, Vec<Library>), EngineError> {
    let mut stmt = conn.prepare("SELECT path FROM files WHERE lang = 'project' ORDER BY path")?;
    let paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?
        .into_iter()
        .filter(|path| scope.contains(path))
        .collect();
    let mut packages: BTreeSet<String> = BTreeSet::new();
    let mut libraries: BTreeMap<String, Library> = BTreeMap::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(repo.store.layout.root.join(&path)) else {
            continue;
        };
        let Some(facts) = project_facts(&path, &text) else {
            continue;
        };
        packages.extend(facts.packages);
        for reference in facts.references {
            if scope.contains(&reference.path) {
                continue;
            }
            let folder = reference.path.rsplit_once('/').map_or(".", |(dir, _)| dir).to_string();
            let component = elements
                .iter()
                .find(|e| e.owns(&reference.path))
                .map(|e| e.label.clone());
            libraries.entry(folder.clone()).or_insert(Library {
                name: reference.name,
                folder,
                component,
            });
        }
    }
    Ok((packages.into_iter().collect(), libraries.into_values().collect()))
}

/// What a component or a module holds and touches. A target starting with `mod:` is a module id.
pub fn explain(repo: &Repo, q: &CodeQuery) -> Result<Explain, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let snapshot = repo.snapshot()?;
    let elements = elements_of(&snapshot);
    let all_modules = stored_modules(conn)?;
    let scope = if q.target.starts_with("mod:") {
        let Some(module) = all_modules.iter().find(|m| m.id == q.target) else {
            return Err(EngineError::Invalid(format!(
                "no module `{}`; module ids look like mod:<project>/<folder>/<k> and change when the code does",
                q.target
            )));
        };
        Scope::Module(module)
    } else {
        match snapshot.resolve(&q.target) {
            Ok((entry, _)) => {
                let Some(element) = elements.iter().find(|e| e.id == entry.id.as_str()) else {
                    return Err(EngineError::Invalid(format!(
                        "`{}` is not a component with a code list; give a component id such as arch/core, a module id or a folder",
                        q.target
                    )));
                };
                Scope::Component(element)
            }
            Err(err) => {
                let folder = q
                    .target
                    .trim()
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .replace('\\', "/");
                if folder.is_empty() || !repo.store.layout.root.join(&folder).is_dir() {
                    return Err(err);
                }
                Scope::Folder(folder)
            }
        }
    };
    let label = match &scope {
        Scope::Component(element) => element.label.clone(),
        Scope::Module(module) => format!("{} ({})", module.id, module.label),
        Scope::Folder(folder) => format!("{folder}/"),
    };

    // The component's files, with their line counts.
    let mut stmt = conn.prepare(
        "SELECT f.id, f.path, f.lang, ifnull((SELECT max(line_end) FROM symbols WHERE file_id = f.id), 1) \
         FROM files f WHERE f.lang IS NOT NULL ORDER BY f.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, u32>(3)?,
        ))
    })?;
    let mut file_ids: Vec<i64> = Vec::new();
    let mut paths: BTreeSet<String> = BTreeSet::new();
    let mut files: Vec<Hit> = Vec::new();
    for row in rows {
        let (id, path, lang, lines) = row?;
        if scope.contains(&path) {
            file_ids.push(id);
            paths.insert(path.clone());
            files.push(Hit {
                id: path.clone(),
                anchor: None,
                path,
                lines: [1, lines],
                kind: "file".to_string(),
                snippet: lang,
                exactness: None,
            });
        }
    }

    // The inside view groups a component's or folder's files by their top folder; a module is
    // already one group.
    let root: Option<String> = match &scope {
        Scope::Component(_) => Some(common_folder(&paths.iter().cloned().collect::<Vec<_>>())),
        Scope::Folder(folder) => Some(folder.clone()),
        Scope::Module(_) => None,
    };
    let mut between: BTreeMap<(String, String), (u32, u32)> = BTreeMap::new();
    let folders: Vec<FolderRow> = match &root {
        Some(root) if !paths.is_empty() => {
            let mut counts: BTreeMap<String, u32> = BTreeMap::new();
            for path in &paths {
                *counts.entry(top_folder(root, path)).or_default() += 1;
            }
            // One folder is no inside view; the file list says the same.
            if counts.len() < 2 {
                Vec::new()
            } else {
                counts
                    .into_iter()
                    .map(|(folder, files)| FolderRow { folder, files })
                    .collect()
            }
        }
        _ => Vec::new(),
    };

    let mut key_symbols: Vec<Hit> = Vec::new();
    let mut qnames: BTreeSet<String> = BTreeSet::new();
    let mut inbound: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut outbound: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    // Element ids on the other side of a code edge, either way.
    let mut linked: BTreeSet<String> = BTreeSet::new();
    if !file_ids.is_empty() {
        let inside = id_list(&file_ids);
        // `edge_ends` is a view: `dst_id IS NOT NULL` keeps the unresolved edges out before its
        // native mask runs. The joins drop them anyway, but only after paying for the mask.
        let mut stmt = conn.prepare(&format!(
            "SELECT d.id, count(*) AS n FROM edge_ends e \
             JOIN symbols d ON d.id = e.dst_id JOIN symbols s ON s.id = e.src_id \
             WHERE e.dst_id IS NOT NULL AND d.file_id IN ({inside}) AND s.file_id != d.file_id AND d.kind != 'file' \
             GROUP BY d.id ORDER BY n DESC, d.qname, d.id LIMIT {KEY_SYMBOLS}"
        ))?;
        let top: Vec<(i64, u32)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, degree) in top {
            let s = sym(conn, id)?;
            key_symbols.push(hit_of(&s, true, &format!("in {degree}")));
        }
        let mut stmt = conn.prepare(&format!("SELECT qname FROM symbols WHERE file_id IN ({inside})"))?;
        qnames.extend(
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        );

        let mut stmt = conn.prepare(&format!(
            "SELECT sf.path, df.path, e.exact FROM edge_ends e \
             JOIN symbols s ON s.id = e.src_id JOIN files sf ON sf.id = s.file_id \
             JOIN symbols d ON d.id = e.dst_id JOIN files df ON df.id = d.file_id \
             WHERE e.dst_id IS NOT NULL AND s.file_id != d.file_id AND e.kind <> 'dispatch' \
             AND (s.file_id IN ({inside}) OR d.file_id IN ({inside}))"
        ))?;
        let edges: Vec<(String, String, bool)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)))?
            .collect::<Result<_, _>>()?;
        // A component's or folder's neighbours are components, a module's are modules.
        let other_side = |path: &str| -> String {
            match &scope {
                Scope::Component(element) => elements
                    .iter()
                    .find(|e| e.id != element.id && e.owns(path))
                    .map(|e| e.label.clone()),
                Scope::Folder(_) => elements.iter().find(|e| e.owns(path)).map(|e| e.label.clone()),
                Scope::Module(_) => all_modules
                    .iter()
                    .find(|m| m.files.binary_search_by(|f| f.as_str().cmp(path)).is_ok())
                    .map(|m| m.id.clone()),
            }
            .unwrap_or_else(|| "(unowned)".to_string())
        };
        for (src, dst, exact) in edges {
            if paths.contains(&src) && paths.contains(&dst) {
                if let Some(root) = &root {
                    let (from, to) = (top_folder(root, &src), top_folder(root, &dst));
                    if from != to {
                        let count = between.entry((from, to)).or_default();
                        if exact {
                            count.0 += 1;
                        } else {
                            count.1 += 1;
                        }
                    }
                }
                continue;
            }
            let (other, counts) = if paths.contains(&src) {
                (&dst, &mut outbound)
            } else {
                (&src, &mut inbound)
            };
            // Tests, benchmarks and generated code on the other side are not a dependency of the design.
            if is_support_path(other) {
                continue;
            }
            linked.extend(elements.iter().filter(|e| e.owns(other)).map(|e| e.id.clone()));
            let side = other_side(other);
            let count = counts.entry(side).or_default();
            if exact {
                count.0 += 1;
            } else {
                count.1 += 1;
            }
        }
    }
    let dependencies = |map: BTreeMap<String, (u32, u32)>| -> Vec<Dependency> {
        let mut list: Vec<Dependency> = map
            .into_iter()
            .map(|(component, (exact, inferred))| Dependency {
                component,
                exact,
                inferred,
            })
            .collect();
        list.sort_by(|a, b| {
            (b.exact + b.inferred)
                .cmp(&(a.exact + a.inferred))
                .then_with(|| a.component.cmp(&b.component))
        });
        list
    };

    let mut folder_edges: Vec<FolderEdge> = between
        .into_iter()
        .map(|((from, to), (exact, inferred))| FolderEdge {
            from,
            to,
            exact,
            inferred,
        })
        .collect();
    folder_edges.sort_by(|a, b| (b.exact + b.inferred).cmp(&(a.exact + a.inferred)));
    let (packages, libraries) = match &scope {
        Scope::Module(_) => (Vec::new(), Vec::new()),
        _ => project_view(repo, conn, &scope, &elements)?,
    };

    let owners: BTreeSet<String> = match &scope {
        Scope::Component(element) => BTreeSet::from([element.id.clone()]),
        Scope::Module(_) | Scope::Folder(_) => paths.iter().flat_map(|path| owners_of(&elements, path)).collect(),
    };
    let (tasks, docs) = related(&snapshot, conn, &owners, &paths, &qnames)?;

    let item = match &scope {
        Scope::Component(element) => snapshot
            .entries
            .iter()
            .find(|e| e.id.as_str() == element.id)
            .and_then(|e| e.item.as_ref()),
        Scope::Module(_) | Scope::Folder(_) => None,
    };
    let declared = item.map(|item| item.fields.entry.clone()).unwrap_or_default();
    let found = detect(conn, &repo.store.layout.root, &|path| scope.contains(path))?;
    let entries = component_rows(conn, &found, &declared, &|path| scope.contains(path))?;
    let uses: Vec<UseLink> = item
        .map(|item| {
            uses_bullets(&item.body, item.body_line)
                .into_iter()
                .filter(|bullet| !bullet.description.is_empty())
                .map(|bullet| UseLink {
                    target: snapshot
                        .resolve(&bullet.target)
                        .map_or(bullet.target, |(entry, _)| snapshot.label(entry)),
                    description: bullet.description,
                })
                .collect()
        })
        .unwrap_or_default();

    let key_count = key_symbols.len();
    let (kept, omitted, left_out) = cut(
        key_symbols.into_iter().chain(files).collect(),
        KEY_SYMBOLS + effective_limit(q.limit),
        q.budget,
    );
    let (key_symbols, files): (Vec<Hit>, Vec<Hit>) = {
        let split = kept.len().min(key_count);
        let mut kept = kept;
        let files = kept.split_off(split);
        (kept, files)
    };
    Ok(Explain {
        target: label,
        entries,
        uses,
        files,
        key_symbols,
        inbound: dependencies(inbound),
        outbound: dependencies(outbound),
        tasks,
        docs,
        truncated: omitted > 0,
        omitted,
        left_out,
        note: building_note(conn)?,
        folders,
        folder_edges,
        packages,
        libraries,
    })
}
