//! Rust resolution. A name resolves through the module tree: the items of the
//! module, then its `use` imports (following `pub use` chains and globs), and
//! for a first path segment also the crates of the workspace. Path calls,
//! types, trait impls and `self.f()` inside an impl are exact when they end on
//! one definition. Any other method call `x.f()` is inferred: every method
//! named `f` in the crate (or, failing that, the crates it depends on per
//! Cargo.toml) is a target. A crate name only resolves to a crate it depends on.
//!
//! Visibility is not tracked: every `use` can be followed as a re-export.
//! A module reached through `#[path]` makes its edges inferred.

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{DeclKind, ImportDecl, Lang, RefKind, RefSite};
use crate::resolve::{Bucket, Edge, Node, Resolver, SymRef, World, finish};
use crate::rust_modules::{Tree, build};

pub(crate) struct RustResolver;

impl Resolver for RustResolver {
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge> {
        let index = Index::build(world);
        let mut edges = Vec::new();
        for &file in files {
            if world.files().get(file).is_some_and(|facts| facts.lang == Lang::Rust) {
                index.resolve_file(file, &mut edges);
            }
        }
        finish(edges)
    }

    fn bucket_members(&self, world: &World, buckets: &[Bucket]) -> Vec<Vec<SymRef>> {
        let index = Index::build(world);
        let by_dir: BTreeMap<&str, usize> = index
            .tree
            .crates
            .iter()
            .enumerate()
            .map(|(krate, krate_info)| (krate_info.dir.as_str(), krate))
            .collect();
        buckets
            .iter()
            .map(|bucket| {
                let Some(&krate) = by_dir.get(bucket.scope.as_str()) else {
                    return Vec::new();
                };
                let all = index
                    .methods_by_name
                    .get(bucket.name.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                index.nearest_crate(krate, all)
            })
            .collect()
    }
}

const MAX_DEPTH: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Module(usize),
    Sym(SymRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Found {
    target: Target,
    inferred: bool,
}

type Seen = BTreeSet<(usize, String)>;

struct Index<'w> {
    world: &'w World,
    tree: Tree,
    /// Per module node, its named items.
    items: Vec<BTreeMap<&'w str, Vec<SymRef>>>,
    /// Per module node, the `use` leaves written in it.
    uses: Vec<Vec<&'w ImportDecl>>,
    /// Methods, associated consts and variants by owning type or trait.
    members: BTreeMap<SymRef, BTreeMap<&'w str, Vec<SymRef>>>,
    /// The traits a type implements.
    traits_of: BTreeMap<SymRef, Vec<SymRef>>,
    methods_by_name: BTreeMap<&'w str, Vec<SymRef>>,
    macros: BTreeMap<&'w str, Vec<SymRef>>,
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

impl<'w> Index<'w> {
    fn build(world: &'w World) -> Self {
        let tree = build(world);
        let mut index = Index {
            world,
            items: vec![BTreeMap::new(); tree.nodes.len()],
            uses: vec![Vec::new(); tree.nodes.len()],
            members: BTreeMap::new(),
            traits_of: BTreeMap::new(),
            methods_by_name: BTreeMap::new(),
            macros: BTreeMap::new(),
            tree,
        };
        index.collect_items();
        index.collect_members();
        index.collect_traits();
        index
    }

    fn collect_items(&mut self) {
        let world = self.world;
        for (n, node) in self.tree.nodes.iter().enumerate() {
            let facts = &world.files()[node.file];
            for (d, decl) in facts.decls.iter().enumerate() {
                let direct = match node.decl {
                    Some(s) => decl.parent == Some(s),
                    None => matches!(decl.parent, None | Some(0)),
                };
                if direct
                    && matches!(
                        decl.kind,
                        DeclKind::Type | DeclKind::Function | DeclKind::Member | DeclKind::Module
                    )
                {
                    self.items[n].entry(decl.name.as_str()).or_default().push(SymRef {
                        file: node.file,
                        decl: d,
                    });
                }
            }
        }
        for (f, facts) in world.files().iter().enumerate() {
            for import in &facts.imports {
                if let Some(n) = self.tree.node_at(f, import.line) {
                    self.uses[n].push(import);
                }
            }
            for (d, decl) in facts.decls.iter().enumerate() {
                if decl.kind == DeclKind::Function && decl.signature.starts_with("macro_rules!") {
                    self.macros
                        .entry(decl.name.as_str())
                        .or_default()
                        .push(SymRef { file: f, decl: d });
                }
            }
        }
    }

    fn collect_members(&mut self) {
        let world = self.world;
        for (f, facts) in world.files().iter().enumerate() {
            if facts.lang != Lang::Rust {
                continue;
            }
            for (d, decl) in facts.decls.iter().enumerate() {
                if !matches!(decl.kind, DeclKind::Method | DeclKind::Member) {
                    continue;
                }
                let Some(parent) = decl.parent else { continue };
                let sym = SymRef { file: f, decl: d };
                if decl.kind == DeclKind::Method {
                    self.methods_by_name.entry(decl.name.as_str()).or_default().push(sym);
                }
                let owner = if facts.decls[parent].kind == DeclKind::Type {
                    Some(SymRef { file: f, decl: parent })
                } else if decl.qname.contains("::") {
                    self.owner_by_name(f, decl.line_start, &decl.qname)
                } else {
                    None
                };
                if let Some(owner) = owner {
                    self.members
                        .entry(owner)
                        .or_default()
                        .entry(decl.name.as_str())
                        .or_default()
                        .push(sym);
                }
            }
        }
    }

    /// The type an impl method belongs to when the type is not in the same
    /// module: the second-to-last segment of its qualified name, looked up
    /// from where the impl sits.
    fn owner_by_name(&self, file: usize, line: u32, qname: &str) -> Option<SymRef> {
        let owner = qname.rsplit("::").nth(1)?;
        let node = self.tree.node_at(file, line)?;
        self.lookup(node, owner, &mut Seen::new(), 0)
            .into_iter()
            .find_map(|found| match found.target {
                Target::Sym(sym) if self.world.decl(sym).kind == DeclKind::Type => Some(sym),
                _ => None,
            })
    }

    fn collect_traits(&mut self) {
        let world = self.world;
        for (f, facts) in world.files().iter().enumerate() {
            for site in &facts.refs {
                if site.kind != RefKind::Implements {
                    continue;
                }
                let Some(scope) = site.scope else { continue };
                if facts.decls[scope].kind != DeclKind::Type {
                    continue;
                }
                let Some(node) = self.tree.node_at(f, site.line) else {
                    continue;
                };
                let segments = site_segments(site);
                for found in self.walk(node, &segments, None, &mut Seen::new(), 0) {
                    if let Target::Sym(tr) = found.target {
                        self.traits_of
                            .entry(SymRef { file: f, decl: scope })
                            .or_default()
                            .push(tr);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ names

    fn target_of(&self, sym: SymRef) -> Target {
        if self.world.decl(sym).kind == DeclKind::Module
            && let Some(&node) = self.tree.by_decl.get(&sym)
        {
            return Target::Module(node);
        }
        Target::Sym(sym)
    }

    fn sym_of(&self, target: Target) -> SymRef {
        match target {
            Target::Module(node) => self.tree.nodes[node].sym,
            Target::Sym(sym) => sym,
        }
    }

    fn lookup(&self, node: usize, name: &str, seen: &mut Seen, depth: u8) -> BTreeSet<Found> {
        let key = (node, name.to_string());
        if depth > MAX_DEPTH || !seen.insert(key.clone()) {
            return BTreeSet::new();
        }
        let found = self.lookup_inner(node, name, seen, depth);
        seen.remove(&key);
        found
    }

    fn lookup_inner(&self, node: usize, name: &str, seen: &mut Seen, depth: u8) -> BTreeSet<Found> {
        let mut out = BTreeSet::new();
        let inferred = self.tree.nodes[node].inferred;
        if let Some(syms) = self.items[node].get(name) {
            for &sym in syms {
                out.insert(Found {
                    target: self.target_of(sym),
                    inferred,
                });
            }
            return out;
        }
        for import in &self.uses[node] {
            let local = import.alias.as_deref().unwrap_or_else(|| last_segment(&import.path));
            if import.glob || local != name {
                continue;
            }
            let segments: Vec<&str> = import.path.split("::").collect();
            for found in self.walk(node, &segments, None, seen, depth + 1) {
                out.insert(Found {
                    target: found.target,
                    inferred: found.inferred || inferred,
                });
            }
        }
        if !out.is_empty() {
            return out;
        }
        for import in &self.uses[node] {
            if !import.glob {
                continue;
            }
            let segments: Vec<&str> = import.path.split("::").collect();
            for found in self.walk(node, &segments, None, seen, depth + 1) {
                match found.target {
                    Target::Module(inner) => {
                        for hit in self.lookup(inner, name, seen, depth + 1) {
                            out.insert(Found {
                                target: hit.target,
                                inferred: hit.inferred || found.inferred || inferred,
                            });
                        }
                    }
                    Target::Sym(owner) => {
                        for &sym in self.members.get(&owner).and_then(|m| m.get(name)).into_iter().flatten() {
                            out.insert(Found {
                                target: Target::Sym(sym),
                                inferred: found.inferred || inferred,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// Resolve a whole path as written in module `node`.
    fn walk(
        &self,
        node: usize,
        segments: &[&str],
        self_ty: Option<SymRef>,
        seen: &mut Seen,
        depth: u8,
    ) -> BTreeSet<Found> {
        let mut out = BTreeSet::new();
        let Some(&first) = segments.first() else {
            return out;
        };
        let mut rest = &segments[1..];
        let here = &self.tree.nodes[node];
        let mut current: BTreeSet<Found> = BTreeSet::new();
        match first {
            "crate" => {
                current.insert(Found {
                    target: Target::Module(self.tree.crates[here.krate].root),
                    inferred: false,
                });
            }
            "self" => {
                current.insert(Found {
                    target: Target::Module(node),
                    inferred: false,
                });
            }
            "super" => {
                let mut at = node;
                let mut skipped = 0;
                while segments.get(skipped) == Some(&"super") {
                    let Some(parent) = self.tree.nodes[at].parent else {
                        return out;
                    };
                    at = parent;
                    skipped += 1;
                }
                rest = &segments[skipped..];
                current.insert(Found {
                    target: Target::Module(at),
                    inferred: false,
                });
            }
            "Self" => {
                if let Some(ty) = self_ty {
                    current.insert(Found {
                        target: Target::Sym(ty),
                        inferred: false,
                    });
                }
            }
            name => {
                current = self.lookup(node, name, seen, depth + 1);
                if current.is_empty()
                    && let Some(&krate) = self.tree.lib_by_name.get(name)
                    && self.tree.sees(here.krate, krate)
                {
                    current.insert(Found {
                        target: Target::Module(self.tree.crates[krate].root),
                        inferred: false,
                    });
                }
            }
        }
        for &segment in rest {
            let mut next = BTreeSet::new();
            for found in &current {
                match found.target {
                    Target::Module(inner) => {
                        for hit in self.lookup(inner, segment, seen, depth + 1) {
                            next.insert(Found {
                                target: hit.target,
                                inferred: hit.inferred || found.inferred,
                            });
                        }
                    }
                    Target::Sym(owner) => {
                        for &sym in self
                            .members
                            .get(&owner)
                            .and_then(|m| m.get(segment))
                            .into_iter()
                            .flatten()
                        {
                            next.insert(Found {
                                target: Target::Sym(sym),
                                inferred: found.inferred,
                            });
                        }
                    }
                }
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }
        out.extend(current);
        out
    }

    /// The type an impl method or trait default method sits in.
    fn self_type(&self, file: usize, scope: Option<usize>, node: usize) -> Option<SymRef> {
        let decls = &self.world.files()[file].decls;
        let mut current = scope;
        while let Some(d) = current {
            let decl = decls.get(d)?;
            match decl.kind {
                DeclKind::Type => return Some(SymRef { file, decl: d }),
                DeclKind::Method => {
                    if let Some(parent) = decl.parent
                        && decls.get(parent)?.kind == DeclKind::Type
                    {
                        return Some(SymRef { file, decl: parent });
                    }
                    let owner = decl.qname.rsplit("::").nth(1)?;
                    return self
                        .lookup(node, owner, &mut Seen::new(), 0)
                        .into_iter()
                        .find_map(|found| match found.target {
                            Target::Sym(sym) if self.world.decl(sym).kind == DeclKind::Type => Some(sym),
                            _ => None,
                        });
                }
                _ => current = decl.parent.filter(|&parent| parent != 0),
            }
        }
        None
    }

    /// Members of `ty` named `name`, then those of the traits it implements.
    fn member_of(&self, ty: SymRef, name: &str) -> Vec<SymRef> {
        if let Some(list) = self.members.get(&ty).and_then(|m| m.get(name)) {
            return list.clone();
        }
        let mut out = Vec::new();
        for tr in self.traits_of.get(&ty).into_iter().flatten() {
            if let Some(list) = self.members.get(tr).and_then(|m| m.get(name)) {
                out.extend(list.iter().copied());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    // ------------------------------------------------------------ sites

    fn resolve_file(&self, file: usize, edges: &mut Vec<Edge>) {
        let world = self.world;
        for site in &world.files()[file].refs {
            let Some(node) = self.tree.node_at(file, site.line) else {
                continue;
            };
            let src = Node::Decl(SymRef {
                file,
                decl: site.scope.unwrap_or(0),
            });
            let site_inferred = self.tree.nodes[node].inferred;
            let self_ty = self.self_type(file, site.scope, node);
            let outcome = match site.kind {
                RefKind::Calls => self.resolve_call(file, node, site, self_ty),
                RefKind::Imports | RefKind::References => {
                    let found = self.walk(node, &site_segments(site), self_ty, &mut Seen::new(), 0);
                    if found.is_empty() {
                        continue;
                    }
                    Outcome::Found(found.into_iter().collect(), true)
                }
                RefKind::Inherits | RefKind::Implements => {
                    let found = self.walk(node, &site_segments(site), self_ty, &mut Seen::new(), 0);
                    if found.is_empty() {
                        Outcome::Unresolved
                    } else {
                        Outcome::Found(found.into_iter().collect(), true)
                    }
                }
                // Only the dispatch pass makes this, from resolved edges.
                RefKind::Dispatch => Outcome::Skip,
            };
            match outcome {
                Outcome::Skip => {}
                Outcome::Unresolved => edges.push(Edge {
                    src,
                    dst: None,
                    dst_name: written(site),
                    kind: site.kind,
                    exact: false,
                    line: site.line,
                }),
                Outcome::Found(found, may_be_exact) => {
                    let single = found.len() == 1;
                    for hit in found {
                        let target = self.sym_of(hit.target);
                        edges.push(Edge {
                            src: src.clone(),
                            dst: Some(Node::Decl(target)),
                            dst_name: world.decl(target).qname.clone(),
                            kind: site.kind,
                            exact: may_be_exact && single && !hit.inferred && !site_inferred,
                            line: site.line,
                        });
                    }
                }
                Outcome::Bucket(bucket) => {
                    let dst_name = bucket.qname();
                    edges.push(Edge {
                        src,
                        dst: Some(Node::Bucket(bucket)),
                        dst_name,
                        kind: site.kind,
                        exact: false,
                        line: site.line,
                    });
                }
            }
        }
    }

    fn resolve_call(&self, file: usize, node: usize, site: &RefSite, self_ty: Option<SymRef>) -> Outcome {
        let qualifier = site.qualifier.as_deref();
        if qualifier.is_some_and(|q| q.starts_with('!')) {
            return self.resolve_macro(file, &site.name);
        }
        if let Some(receiver) = qualifier.and_then(|q| q.strip_prefix('.')) {
            if receiver == "self"
                && let Some(ty) = self_ty
            {
                let own = self.member_of(ty, &site.name);
                if !own.is_empty() {
                    return Outcome::Found(own.into_iter().map(exact_sym).collect(), true);
                }
            }
            return self.infer_method(file, &site.name);
        }
        let found = self.walk(node, &site_segments(site), self_ty, &mut Seen::new(), 0);
        if !found.is_empty() {
            return Outcome::Found(found.into_iter().collect(), true);
        }
        // `Some(x)`, `Ok(x)`: constructors of the standard library say nothing.
        if qualifier.is_none() && site.name.starts_with(|c: char| c.is_uppercase()) {
            return Outcome::Skip;
        }
        Outcome::Unresolved
    }

    fn resolve_macro(&self, file: usize, name: &str) -> Outcome {
        let Some(all) = self.macros.get(name) else {
            return Outcome::Skip;
        };
        let chosen = self.nearest(file, all);
        if chosen.is_empty() {
            return Outcome::Skip;
        }
        Outcome::Found(chosen.into_iter().map(exact_sym).collect(), true)
    }

    /// The candidates in the crate of `file`, else those in the crates it
    /// depends on. Crates it cannot see never count.
    fn nearest(&self, file: usize, all: &[SymRef]) -> Vec<SymRef> {
        let Some(krate) = self.tree.crate_of_file(file) else {
            return Vec::new();
        };
        self.nearest_crate(krate, all)
    }

    /// [`Self::nearest`] for a crate rather than a file of it.
    fn nearest_crate(&self, krate: usize, all: &[SymRef]) -> Vec<SymRef> {
        let same: Vec<SymRef> = all
            .iter()
            .copied()
            .filter(|sym| self.tree.crate_of_file(sym.file) == Some(krate))
            .collect();
        if !same.is_empty() {
            return same;
        }
        all.iter()
            .copied()
            .filter(|sym| {
                self.tree
                    .crate_of_file(sym.file)
                    .is_some_and(|other| self.tree.sees(krate, other))
            })
            .collect()
    }

    fn infer_method(&self, file: usize, name: &str) -> Outcome {
        let Some(all) = self.methods_by_name.get(name) else {
            return Outcome::Unresolved;
        };
        let chosen = self.nearest(file, all);
        match chosen.as_slice() {
            [] => Outcome::Unresolved,
            [_one] => Outcome::Found(chosen.into_iter().map(exact_sym).collect(), false),
            _ => {
                let Some(krate) = self.tree.crate_of_file(file) else {
                    return Outcome::Unresolved;
                };
                let scope = self.tree.crates[krate].dir.clone();
                Outcome::Bucket(Bucket::new(scope, name))
            }
        }
    }
}

fn exact_sym(sym: SymRef) -> Found {
    Found {
        target: Target::Sym(sym),
        inferred: false,
    }
}

enum Outcome {
    /// Nothing to say: standard-library names and unknown macros.
    Skip,
    Unresolved,
    /// Targets, and whether one target may count as exact.
    Found(Vec<Found>, bool),
    /// Several fallback candidates behind one name bucket.
    Bucket(Bucket),
}

/// The path segments of a site: its qualifier then its name.
fn site_segments(site: &RefSite) -> Vec<&str> {
    let mut segments: Vec<&str> = match site.qualifier.as_deref() {
        Some(q) if !q.starts_with(['.', '!']) => q.split("::").filter(|part| !part.is_empty()).collect(),
        _ => Vec::new(),
    };
    segments.push(site.name.as_str());
    segments
}

/// The target as written, for an edge that found nothing.
fn written(site: &RefSite) -> String {
    match site.qualifier.as_deref() {
        Some(q) if !q.starts_with(['.', '!']) => format!("{q}::{}", site.name),
        _ => site.name.clone(),
    }
}
