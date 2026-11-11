//! `stone arch c4`: the C4 context, container and component diagrams as Mermaid text, drawn from
//! the arch items and their connections. Agents edit the data; the text is never kept.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::arch_graph::{ArchNode, UsesEdge, arch_nodes, component_edges, target_id};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// A generated diagram: what `stone arch c4 --json` and `/api/arch/c4/*` return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct C4Source {
    /// The Mermaid C4 text.
    pub source: String,
}

/// The arch items with their parents resolved and their `uses` entries between them.
struct Model {
    nodes: BTreeMap<String, ArchNode>,
    /// Item id to the id of the arch item its `parent` names.
    parents: BTreeMap<String, String>,
    /// Sorted by `from`, then `to`; both ends are arch items.
    uses: Vec<UsesEdge>,
}

/// One line of a diagram, before it is written.
struct Line {
    label: String,
    technology: Option<String>,
}

impl Model {
    fn new(snapshot: &Snapshot) -> Model {
        let (list, mut uses) = arch_nodes(snapshot);
        let nodes: BTreeMap<String, ArchNode> = list.into_iter().map(|n| (n.id.clone(), n)).collect();
        let parents = nodes
            .values()
            .filter_map(|n| {
                let parent = target_id(snapshot, n.parent.as_deref()?);
                nodes.contains_key(&parent).then(|| (n.id.clone(), parent))
            })
            .collect();
        uses.retain(|u| nodes.contains_key(&u.from) && nodes.contains_key(&u.to));
        uses.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
        Model { nodes, parents, uses }
    }

    /// The id, its parent, the parent's parent and so on. A `parent` loop ends the walk at the first
    /// repeated id, because `stone check` allows one.
    fn chain<'a>(&'a self, id: &'a str) -> Vec<&'a str> {
        self.walk(id).0
    }

    /// The top of the chain; for a loop, the smallest id in it, so every member agrees.
    fn root<'a>(&'a self, id: &'a str) -> &'a str {
        let (path, looped) = self.walk(id);
        match looped {
            Some(at) => path[at..].iter().min().copied().unwrap_or(id),
            None => path.last().copied().unwrap_or(id),
        }
    }

    /// The chain, and where in it the walk met an id again.
    fn walk<'a>(&'a self, id: &'a str) -> (Vec<&'a str>, Option<usize>) {
        let mut path: Vec<&str> = Vec::new();
        let mut current = id;
        loop {
            if let Some(at) = path.iter().position(|p| *p == current) {
                return (path, Some(at));
            }
            path.push(current);
            match self.parents.get(current) {
                Some(parent) => current = parent.as_str(),
                None => return (path, None),
            }
        }
    }

    fn type_of(&self, id: &str) -> &str {
        self.nodes.get(id).map_or("", |n| n.type_name.as_str())
    }

    fn kind_of(&self, id: &str) -> Option<&str> {
        self.nodes.get(id).and_then(|n| n.kind.as_deref())
    }

    fn name(&self, id: &str) -> String {
        self.nodes
            .get(id)
            .and_then(|n| n.title.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// The boxes of the context diagram: the top system of each product, person and outside system.
    fn context_boxes(&self) -> BTreeSet<String> {
        self.nodes
            .values()
            .filter(|n| n.type_name == "system")
            .map(|n| self.root(&n.id))
            .filter(|top| self.type_of(top) == "system")
            .map(str::to_string)
            .collect()
    }

    /// Each system with no kind that tops its chain.
    fn focus(&self) -> Vec<String> {
        self.context_boxes()
            .into_iter()
            .filter(|id| self.kind_of(id).is_none())
            .collect()
    }

    /// The topmost id on `id`'s chain that is not on `skip`: the part of the other side that sits
    /// right below the point where the two chains meet.
    fn outside(&self, id: &str, skip: &BTreeSet<&str>) -> Option<String> {
        self.chain(id)
            .into_iter()
            .rev()
            .find(|c| !skip.contains(c))
            .map(str::to_string)
    }

    /// The written lines between the boxes `lift` maps the ends to, for pairs `keep` accepts.
    fn written(
        &self,
        lift: &dyn Fn(&str) -> Option<String>,
        keep: &dyn Fn(&str, &str) -> bool,
    ) -> BTreeMap<(String, String), Line> {
        let mut grouped: BTreeMap<(String, String), Vec<&UsesEdge>> = BTreeMap::new();
        for edge in &self.uses {
            let (Some(from), Some(to)) = (lift(&edge.from), lift(&edge.to)) else {
                continue;
            };
            if from != to && keep(&from, &to) {
                grouped.entry((from, to)).or_default().push(edge);
            }
        }
        grouped
            .into_iter()
            .map(|((from, to), group)| {
                // A connection with a phrase written between these very boxes wins over the ones
                // lifted into them; one with no phrase says nothing, so the lifted ones speak.
                let line = match group
                    .iter()
                    .find(|e| e.from == from && e.to == to && e.description.is_some())
                {
                    Some(own) => Line {
                        label: own.description.clone().unwrap_or_default(),
                        technology: own.technology.clone(),
                    },
                    None => Line {
                        label: distinct(group.iter().filter_map(|e| e.description.as_deref())),
                        technology: Some(distinct(group.iter().filter_map(|e| e.technology.as_deref())))
                            .filter(|t| !t.is_empty()),
                    },
                };
                ((from, to), line)
            })
            .collect()
    }

    /// One mermaid element line: `Container(api, "Api", "Rust", "Serves orders", $link="arch/api")`.
    fn element(&self, id: &str, alias: &str, indent: &str) -> String {
        let node = &self.nodes[id];
        let (name, has_technology) = match (node.type_name.as_str(), node.kind.as_deref()) {
            ("system", Some("person")) => ("Person", false),
            ("system", Some("external")) => ("System_Ext", false),
            ("system", _) => ("System", false),
            ("container", Some("store")) => ("ContainerDb", true),
            ("container", _) => ("Container", true),
            _ => ("Component", true),
        };
        let mut args = vec![alias.to_string(), quoted(node.title.as_deref().unwrap_or(id))];
        let description = node.description.as_deref();
        if has_technology {
            if node.technology.is_some() || description.is_some() {
                args.push(quoted(node.technology.as_deref().unwrap_or("")));
            }
            if let Some(description) = description {
                args.push(quoted(description));
            }
        } else if let Some(description) = description {
            args.push(quoted(description));
        }
        args.push(format!("$link={}", quoted(id)));
        format!("{indent}{name}({})\n", args.join(", "))
    }
}

/// Whether a diagram with a boundary draws the line: it touches the boundary or a box inside it,
/// and it is not between a box and the boundary the box sits in.
fn drawn(inner: &BTreeSet<String>, boundary: &str, from: &str, to: &str) -> bool {
    let touches = inner.contains(from) || inner.contains(to) || from == boundary || to == boundary;
    let within = (from == boundary && inner.contains(to)) || (to == boundary && inner.contains(from));
    touches && !within
}

/// The distinct texts in order, joined with `, `.
fn distinct<'a>(texts: impl Iterator<Item = &'a str>) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for text in texts {
        if !seen.contains(&text) {
            seen.push(text);
        }
    }
    seen.join(", ")
}

/// `"text"` with each double quote written as Mermaid's own entity, which the viewer reads back.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "#quot;"))
}

/// Mermaid aliases for the ids, by id order; two ids that clean to the same word get a number.
fn aliases(ids: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut used = BTreeSet::new();
    let mut out = BTreeMap::new();
    for id in ids {
        let base: String = id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let mut alias = base.clone();
        let mut n = 2;
        while !used.insert(alias.clone()) {
            alias = format!("{base}_{n}");
            n += 1;
        }
        out.insert(id.clone(), alias);
    }
    out
}

fn rel(aliases: &BTreeMap<String, String>, from: &str, to: &str, line: &Line) -> String {
    let mut args = vec![aliases[from].clone(), aliases[to].clone(), quoted(&line.label)];
    if let Some(technology) = &line.technology {
        args.push(quoted(technology));
    }
    format!("  Rel({})\n", args.join(", "))
}

/// The systems and the lines between them, drawn without a boundary.
fn drawn_flat(
    model: &Model,
    title: &str,
    boxes: &BTreeSet<String>,
    lines: &BTreeMap<(String, String), Line>,
) -> String {
    let names = aliases(boxes);
    let mut out = format!("C4Context\n  title {title}\n");
    for id in boxes {
        out.push_str(&model.element(id, &names[id], "  "));
    }
    for ((from, to), line) in lines {
        out.push_str(&rel(&names, from, to, line));
    }
    out
}

/// A diagram with one boundary: the boxes outside it, the boundary with its inner boxes, the lines.
fn drawn_bounded(
    model: &Model,
    head: (&str, &str, &str),
    boundary: (&str, &str),
    inner: &BTreeSet<String>,
    lines: &BTreeMap<(String, String), Line>,
) -> String {
    let (level, title, boundary_macro) = head;
    let mut all: BTreeSet<String> = inner.clone();
    all.insert(boundary.0.to_string());
    for (from, to) in lines.keys() {
        all.insert(from.clone());
        all.insert(to.clone());
    }
    let names = aliases(&all);
    let mut out = format!("{level}\n  title {title}\n");
    for id in all
        .iter()
        .filter(|id| !inner.contains(*id) && id.as_str() != boundary.0)
    {
        out.push_str(&model.element(id, &names[id], "  "));
    }
    out.push_str(&format!(
        "  {boundary_macro}({}, {}) {{\n",
        names[boundary.0],
        quoted(boundary.1)
    ));
    for id in inner {
        out.push_str(&model.element(id, &names[id], "    "));
    }
    out.push_str("  }\n");
    for ((from, to), line) in lines {
        out.push_str(&rel(&names, from, to, line));
    }
    out
}

/// The C4 context diagram: each product, person and outside system, and how they connect. Several
/// products make a landscape.
pub fn c4_context(repo: &Repo) -> Result<String, EngineError> {
    let model = Model::new(&repo.snapshot()?);
    let focus = model.focus();
    if focus.is_empty() {
        return Err(EngineError::Invalid(
            "this store has no system to draw; add one with `stone new system --title <name>`".to_string(),
        ));
    }
    let title = match focus.as_slice() {
        [only] => format!("System context diagram for {}", model.name(only)),
        _ => "System landscape diagram".to_string(),
    };
    let boxes = model.context_boxes();
    let lift = |id: &str| Some(model.root(id).to_string()).filter(|top| boxes.contains(top));
    let lines = model.written(&lift, &|_, _| true);
    Ok(drawn_flat(&model, &title, &boxes, &lines))
}

/// The C4 container diagram of one system: the containers below it, and the people, outside systems
/// and other products that connect to them. `system` may be left out when the store has one product.
pub fn c4_container(repo: &Repo, system: Option<&str>) -> Result<String, EngineError> {
    let snapshot = repo.snapshot()?;
    let model = Model::new(&snapshot);
    let focus = model.focus();
    let chosen = match system {
        Some(name) => snapshot
            .resolve(name)
            .ok()
            .map(|(entry, _)| entry.id.as_str().to_string())
            .filter(|id| model.type_of(id) == "system" && model.kind_of(id).is_none())
            .ok_or_else(|| {
                EngineError::Invalid(format!(
                    "`{name}` is not a system with no kind; the systems are: {}",
                    list(
                        &model
                            .nodes
                            .values()
                            .filter(|n| n.type_name == "system" && n.kind.is_none())
                            .map(|n| n.id.clone())
                            .collect::<Vec<_>>()
                    )
                ))
            })?,
        None => match focus.as_slice() {
            [only] => only.clone(),
            [] => {
                return Err(EngineError::Invalid(
                    "this store has no system to draw; add one with `stone new system --title <name>`".to_string(),
                ));
            }
            _ => {
                return Err(EngineError::Invalid(format!(
                    "this store has several systems: {}; pass one",
                    list(&focus)
                )));
            }
        },
    };
    let own = chosen.as_str();
    let skip: BTreeSet<&str> = model.chain(own).into_iter().collect();
    let inner: BTreeSet<String> = model
        .nodes
        .keys()
        .filter(|id| id.as_str() != own && model.type_of(id) == "container" && model.chain(id).contains(&own))
        .cloned()
        .collect();
    let lift = |id: &str| -> Option<String> {
        if id == own {
            return Some(own.to_string());
        }
        let chain = model.chain(id);
        if chain.contains(&own) {
            // Below the system: the container it sits in, or nothing when it is no container's.
            return chain
                .into_iter()
                .take_while(|c| *c != own)
                .find(|c| inner.contains(*c))
                .map(str::to_string);
        }
        model.outside(id, &skip)
    };
    let lines = model.written(&lift, &|from, to| drawn(&inner, own, from, to));
    let title = format!("Container diagram for {}", model.name(own));
    Ok(drawn_bounded(
        &model,
        ("C4Container", &title, "System_Boundary"),
        (own, &model.name(own)),
        &inner,
        &lines,
    ))
}

/// The C4 component diagram of one container: its components, and the parts that connect to them,
/// from the connections written in the store and the references the code has.
pub fn c4_component(repo: &Repo, container: &str) -> Result<String, EngineError> {
    let snapshot = repo.snapshot()?;
    let model = Model::new(&snapshot);
    let Some(own) = snapshot
        .resolve(container)
        .ok()
        .map(|(entry, _)| entry.id.as_str().to_string())
        .filter(|id| model.type_of(id) == "container")
    else {
        let names: Vec<String> = model
            .nodes
            .values()
            .filter(|n| n.type_name == "container")
            .map(|n| n.id.clone())
            .collect();
        return Err(EngineError::Invalid(if names.is_empty() {
            format!(
                "`{container}` is not a container, and this store has no containers; add one with `stone new container`"
            )
        } else {
            format!("`{container}` is not a container; the containers are: {}", list(&names))
        }));
    };
    let own = own.as_str();
    let skip: BTreeSet<&str> = model.chain(own).into_iter().collect();
    let inner: BTreeSet<String> = model
        .nodes
        .values()
        .filter(|n| n.type_name == "component" && model.parents.get(&n.id).is_some_and(|p| p == own))
        .map(|n| n.id.clone())
        .collect();
    let lift = |id: &str| -> Option<String> {
        if id == own {
            return Some(own.to_string());
        }
        let chain = model.chain(id);
        if let Some(at) = chain.iter().position(|c| *c == own) {
            // Below the container: the component right under it, if that is one of its components.
            return at
                .checked_sub(1)
                .map(|i| chain[i])
                .filter(|c| inner.contains(*c))
                .map(str::to_string);
        }
        model.outside(id, &skip)
    };
    let keep = |from: &str, to: &str| drawn(&inner, own, from, to);
    let mut lines = model.written(&lift, &keep);
    // The code says more than anyone wrote: a reference with no written connection still shows.
    let mut counted: BTreeMap<(String, String), u32> = BTreeMap::new();
    for edge in component_edges(repo)? {
        if edge.inferred_only {
            continue;
        }
        let (Some(from), Some(to)) = (lift(&edge.from), lift(&edge.to)) else {
            continue;
        };
        if from != to && keep(&from, &to) {
            *counted.entry((from, to)).or_default() += edge.edges;
        }
    }
    for (pair, n) in counted {
        let label = if n == 1 {
            "1 reference".to_string()
        } else {
            format!("{n} references")
        };
        match lines.get_mut(&pair) {
            // A connection with no phrase has nothing to keep; the references name the line.
            Some(line) if line.label.is_empty() => line.label = label,
            Some(_) => {}
            None => {
                lines.insert(
                    pair,
                    Line {
                        label,
                        technology: None,
                    },
                );
            }
        }
    }
    let title = format!("Component diagram for {}", model.name(own));
    Ok(drawn_bounded(
        &model,
        ("C4Component", &title, "Container_Boundary"),
        (own, &model.name(own)),
        &inner,
        &lines,
    ))
}

fn list(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
