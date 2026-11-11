//! C# resolution. A name resolves in this order: members and nested types of
//! the enclosing types, then (project by project, own project first and then
//! the projects it references) the enclosing namespaces and the usings, and
//! last the name-only fallback, which is always inferred. An edge is exact
//! when that walk ends on one definition without the fallback. A fallback with
//! several candidates points at one name bucket instead of at each of them.
//!
//! A member call `x.M()` is exact when the receiver's declared type is known
//! (typed local, parameter, field, property, `this`, `new T()`, `var x = new
//! T()`) and the type, or a base of it, declares `M`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::csharp_types::{Binding, binding_var, parse_binding};
use crate::facts::{DeclKind, ImportDecl, Lang, RefKind, RefSite};
use crate::project::ProjectKind;
use crate::resolve::{Bucket, Edge, Node, Resolver, SymRef, WORKSPACE_SCOPE, World, finish};

pub(crate) struct CSharpResolver;

impl Resolver for CSharpResolver {
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge> {
        let index = Index::build(world);
        let mut edges = Vec::new();
        for &file in files {
            if world.files().get(file).is_some_and(|facts| facts.lang == Lang::CSharp) {
                index.resolve_file(file, &mut edges);
            }
        }
        finish(edges)
    }

    fn bucket_members(&self, world: &World, buckets: &[Bucket]) -> Vec<Vec<SymRef>> {
        let index = Index::build(world);
        let mut cache: BTreeMap<&str, Vec<SymRef>> = BTreeMap::new();
        buckets
            .iter()
            .map(|bucket| {
                cache
                    .entry(bucket.name.as_str())
                    .or_insert_with(|| index.fallback_candidates(&bucket.name))
                    .clone()
            })
            .collect()
    }

    fn resolve_projects(&self, world: &World) -> Vec<Edge> {
        let by_path = csproj_paths(world);
        let mut edges = Vec::new();
        for (index, project) in world.projects().iter().enumerate() {
            if project.kind != ProjectKind::Csproj {
                continue;
            }
            for reference in &project.references {
                let dst = by_path.get(&reference.path.to_lowercase()).copied();
                edges.push(Edge {
                    src: Node::Project(index),
                    dst: dst.map(Node::Project),
                    dst_name: reference.name.clone(),
                    kind: RefKind::Imports,
                    exact: dst.is_some(),
                    line: 1,
                });
            }
        }
        finish(edges)
    }
}

/// A type is its project plus its qualified name: partial classes in several
/// files share one, the same name in two projects does not.
type Key = (Option<usize>, String);

fn csproj_paths(world: &World) -> BTreeMap<String, usize> {
    world
        .projects()
        .iter()
        .enumerate()
        .filter(|(_, project)| project.kind == ProjectKind::Csproj)
        .map(|(index, project)| (project.path.to_lowercase(), index))
        .collect()
}

/// Per world file, the nearest enclosing `.csproj` (the project-first scope);
/// `None` for a file outside every project and for non-C# files.
pub(crate) fn file_projects(world: &World) -> Vec<Option<usize>> {
    let paths = csproj_paths(world);
    world
        .files()
        .iter()
        .map(|file| {
            if file.lang != Lang::CSharp {
                return None;
            }
            paths
                .values()
                .copied()
                .filter(|&p| contains(&world.projects()[p].root, &file.path))
                .max_by_key(|&p| world.projects()[p].root.len())
        })
        .collect()
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

#[derive(Default)]
struct TypeInfo {
    /// Every part of the type, in file order; the first is the one edges point at.
    defs: Vec<SymRef>,
    members: BTreeMap<String, Vec<SymRef>>,
}

struct Bound {
    scope: usize,
    line: u32,
    binding: Binding,
}

struct Index<'w> {
    world: &'w World,
    file_project: Vec<Option<usize>>,
    /// Per project, the projects whose types it sees, nearest first.
    tiers: BTreeMap<Option<usize>, Vec<Vec<Option<usize>>>>,
    types: BTreeMap<Key, TypeInfo>,
    bases: BTreeMap<Key, Vec<Key>>,
    global_usings: BTreeMap<Option<usize>, Vec<&'w ImportDecl>>,
    bindings: Vec<BTreeMap<&'w str, Vec<Bound>>>,
    members_by_name: BTreeMap<&'w str, Vec<SymRef>>,
    types_by_name: BTreeMap<&'w str, Vec<SymRef>>,
}

impl<'w> Index<'w> {
    fn build(world: &'w World) -> Self {
        let files = world.files();
        let paths = csproj_paths(world);
        let file_project = file_projects(world);

        let mut index = Index {
            world,
            file_project,
            tiers: BTreeMap::new(),
            types: BTreeMap::new(),
            bases: BTreeMap::new(),
            global_usings: BTreeMap::new(),
            bindings: Vec::new(),
            members_by_name: BTreeMap::new(),
            types_by_name: BTreeMap::new(),
        };

        for (f, file) in files.iter().enumerate() {
            let mut bindings: BTreeMap<&str, Vec<Bound>> = BTreeMap::new();
            if file.lang == Lang::CSharp {
                let project = index.file_project[f];
                for (d, decl) in file.decls.iter().enumerate() {
                    let sym = SymRef { file: f, decl: d };
                    match decl.kind {
                        DeclKind::Type => {
                            index
                                .types
                                .entry((project, decl.qname.clone()))
                                .or_default()
                                .defs
                                .push(sym);
                            index.types_by_name.entry(&decl.name).or_default().push(sym);
                        }
                        DeclKind::Method | DeclKind::Member => {
                            let Some(parent) = decl.parent.map(|p| &file.decls[p]) else {
                                continue;
                            };
                            if parent.kind != DeclKind::Type
                                || (decl.kind == DeclKind::Method && decl.name == parent.name)
                            {
                                continue;
                            }
                            index
                                .types
                                .entry((project, parent.qname.clone()))
                                .or_default()
                                .members
                                .entry(decl.name.clone())
                                .or_default()
                                .push(sym);
                            index.members_by_name.entry(&decl.name).or_default().push(sym);
                        }
                        _ => {}
                    }
                }
                for import in file.imports.iter().filter(|import| import.global) {
                    index.global_usings.entry(project).or_default().push(import);
                }
                for site in &file.refs {
                    if let (Some(var), Some(binding), Some(scope)) =
                        (binding_var(site), parse_binding(site), site.scope)
                    {
                        bindings.entry(var).or_default().push(Bound {
                            scope,
                            line: site.line,
                            binding,
                        });
                    }
                }
            }
            index.bindings.push(bindings);
        }

        index.compute_tiers(&paths);
        index.compute_bases();
        index
    }

    fn compute_tiers(&mut self, paths: &BTreeMap<String, usize>) {
        let all: Vec<Option<usize>> = self
            .types
            .keys()
            .map(|key| key.0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.tiers.insert(None, vec![all]);
        for &start in paths.values() {
            let mut seen = BTreeSet::from([start]);
            let mut order = vec![start];
            let mut queue = VecDeque::from([start]);
            while let Some(current) = queue.pop_front() {
                for reference in &self.world.projects()[current].references {
                    if let Some(&next) = paths.get(&reference.path.to_lowercase())
                        && seen.insert(next)
                    {
                        order.push(next);
                        queue.push_back(next);
                    }
                }
            }
            self.tiers
                .insert(Some(start), order.into_iter().map(|p| vec![Some(p)]).collect());
        }
    }

    fn compute_bases(&mut self) {
        let mut bases = BTreeMap::new();
        for (key, info) in &self.types {
            let mut found = Vec::new();
            for def in &info.defs {
                let file = &self.world.files()[def.file];
                let context = file.decls[def.decl].parent;
                for site in &file.refs {
                    if site.scope == Some(def.decl) && matches!(site.kind, RefKind::Inherits | RefKind::Implements) {
                        let candidates = self.lookup_type(def.file, context, &site_path(site));
                        if let [only] = candidates.as_slice()
                            && only != key
                            && !found.contains(only)
                        {
                            found.push(only.clone());
                        }
                    }
                }
            }
            bases.insert(key.clone(), found);
        }
        self.bases = bases;
    }

    // ------------------------------------------------------------ scopes

    fn decl_at(&self, file: usize, index: usize) -> Option<&'w crate::facts::Decl> {
        self.world.files()[file].decls.get(index)
    }

    /// The enclosing types of a scope, innermost first.
    fn enclosing_types(&self, file: usize, scope: Option<usize>) -> Vec<Key> {
        let mut out = Vec::new();
        let mut current = scope;
        while let Some(index) = current {
            let Some(decl) = self.decl_at(file, index) else {
                break;
            };
            if decl.kind == DeclKind::Type {
                out.push((self.file_project[file], decl.qname.clone()));
            }
            current = decl.parent;
        }
        out
    }

    /// The enclosing namespace and each prefix of it, ending with the root.
    fn namespace_chain(&self, file: usize, scope: Option<usize>) -> Vec<String> {
        let mut current = scope;
        let mut namespace = "";
        while let Some(index) = current {
            let Some(decl) = self.decl_at(file, index) else {
                break;
            };
            if decl.kind == DeclKind::Module {
                namespace = &decl.qname;
                break;
            }
            current = decl.parent;
        }
        let mut chain = Vec::new();
        let mut rest = namespace;
        while !rest.is_empty() {
            chain.push(rest.to_string());
            rest = rest.rsplit_once('.').map_or("", |(head, _)| head);
        }
        chain.push(String::new());
        chain
    }

    fn usings(&self, file: usize) -> impl Iterator<Item = &'w ImportDecl> {
        let project = self.file_project[file];
        self.world.files()[file]
            .imports
            .iter()
            .chain(self.global_usings.get(&project).into_iter().flatten().copied())
    }

    fn canonical(&self, key: &Key) -> SymRef {
        self.types[key].defs[0]
    }

    // ------------------------------------------------------------ types

    /// The types `path` names from this scope. One key means resolved, several
    /// mean ambiguous, none mean not in the repository.
    fn lookup_type(&self, file: usize, scope: Option<usize>, path: &str) -> Vec<Key> {
        let project = self.file_project[file];
        let tiers = &self.tiers[&project];
        let (first, rest) = path.split_once('.').map_or((path, ""), |(a, b)| (a, b));

        if let Some(alias) = self.usings(file).find(|import| import.alias.as_deref() == Some(first)) {
            let full = if rest.is_empty() {
                alias.path.clone()
            } else {
                join(&alias.path, rest)
            };
            return Self::in_tiers(&self.types, tiers, [full.as_str()]);
        }

        for (_, qname) in self.enclosing_types(file, scope) {
            if let Some(key) = self.nested_key(project, &qname, path) {
                return vec![key];
            }
        }

        let chain = self.namespace_chain(file, scope);
        let using_paths: Vec<&str> = self
            .usings(file)
            .filter(|import| import.alias.is_none() && !import.is_static)
            .map(|import| import.path.as_str())
            .collect();
        for tier in tiers {
            for namespace in &chain {
                let found = Self::keys_in(&self.types, tier, &join(namespace, path));
                if !found.is_empty() {
                    return found;
                }
            }
            let mut found = Vec::new();
            for using in &using_paths {
                found.extend(Self::keys_in(&self.types, tier, &join(using, path)));
            }
            found.sort();
            found.dedup();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    fn nested_key(&self, project: Option<usize>, outer: &str, path: &str) -> Option<Key> {
        let key = (project, join(outer, path));
        self.types.contains_key(&key).then_some(key)
    }

    fn keys_in(types: &BTreeMap<Key, TypeInfo>, tier: &[Option<usize>], qname: &str) -> Vec<Key> {
        tier.iter()
            .map(|&project| (project, qname.to_string()))
            .filter(|key| types.contains_key(key))
            .collect()
    }

    fn in_tiers<'a>(
        types: &BTreeMap<Key, TypeInfo>,
        tiers: &[Vec<Option<usize>>],
        qnames: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Key> {
        for qname in qnames {
            for tier in tiers {
                let found = Self::keys_in(types, tier, qname);
                if !found.is_empty() {
                    return found;
                }
            }
        }
        Vec::new()
    }

    // ------------------------------------------------------------ members

    /// Members called `name` on a type, or on the nearest base that has any.
    fn find_members(&self, start: &Key, name: &str) -> Vec<SymRef> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start.clone()]);
        while let Some(key) = queue.pop_front() {
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(found) = self.types.get(&key).and_then(|info| info.members.get(name))
                && !found.is_empty()
            {
                return found.clone();
            }
            queue.extend(self.bases.get(&key).into_iter().flatten().cloned());
        }
        Vec::new()
    }

    fn static_using_members(&self, file: usize, name: &str) -> Vec<SymRef> {
        for import in self.usings(file).filter(|import| import.is_static) {
            if let [key] = self.lookup_type(file, None, &import.path).as_slice() {
                let found = self.find_members(key, name);
                if !found.is_empty() {
                    return found;
                }
            }
        }
        Vec::new()
    }

    // ------------------------------------------------------------ receivers

    /// The type behind a binding, `None` when it is not in the repository or ambiguous.
    fn bound_type(&self, file: usize, scope: usize, binding: &Binding) -> Option<Key> {
        match self.lookup_type(file, Some(scope), &binding.type_path()).as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// What the variable `var` is declared as where it is used: outer `None`
    /// is "no such variable", inner `None` is "declared, type unknown".
    fn variable_type(&self, file: usize, scope: Option<usize>, line: u32, var: &str) -> Option<Option<Key>> {
        let mut current = scope;
        while let Some(index) = current {
            let decl = self.decl_at(file, index)?;
            if decl.kind == DeclKind::Type {
                let key = (self.file_project[file], decl.qname.clone());
                if let Some(found) = self.field_type(&key, var) {
                    return Some(found);
                }
            } else if let Some(bound) = self.bindings[file].get(var).and_then(|entries| {
                entries
                    .iter()
                    .filter(|e| e.scope == index && e.line <= line)
                    .max_by_key(|e| e.line)
            }) {
                return Some(self.bound_type(file, index, &bound.binding));
            }
            current = decl.parent;
        }
        None
    }

    /// A field or property of a type (or of its bases), as `variable_type` reports it.
    fn field_type(&self, start: &Key, var: &str) -> Option<Option<Key>> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start.clone()]);
        while let Some(key) = queue.pop_front() {
            if !seen.insert(key.clone()) {
                continue;
            }
            for def in self.types.get(&key).into_iter().flat_map(|info| &info.defs) {
                if let Some(bound) = self.bindings[def.file]
                    .get(var)
                    .and_then(|entries| entries.iter().find(|e| e.scope == def.decl))
                {
                    return Some(self.bound_type(def.file, def.decl, &bound.binding));
                }
            }
            queue.extend(self.bases.get(&key).into_iter().flatten().cloned());
        }
        None
    }

    /// The declared type of a receiver such as `s`, `this`, `this._svc` or `order.Customer`.
    fn receiver_type(&self, file: usize, scope: Option<usize>, line: u32, receiver: &str) -> Option<Key> {
        if receiver.contains(['(', '<', '[', '?', '!']) {
            return None;
        }
        let segments: Vec<&str> = receiver.split('.').collect();
        let mut current = if segments[0] == "this" {
            self.enclosing_types(file, scope).into_iter().next()?
        } else if let Some(declared) = self.variable_type(file, scope, line, segments[0]) {
            declared?
        } else {
            return match self.lookup_type(file, scope, receiver).as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            };
        };
        for segment in &segments[1..] {
            current = match self.field_type(&current, segment) {
                Some(declared) => declared?,
                None => self.nested_key(current.0, &current.1, segment)?,
            };
        }
        Some(current)
    }

    // ------------------------------------------------------------ sites

    fn resolve_file(&self, file: usize, edges: &mut Vec<Edge>) {
        for site in &self.world.files()[file].refs {
            let src = || {
                Node::Decl(SymRef {
                    file,
                    decl: site.scope.unwrap_or(0),
                })
            };
            let (targets, exact) = match site.kind {
                RefKind::Calls => self.resolve_call(file, site),
                RefKind::Inherits | RefKind::Implements => {
                    let (found, exact) = self.resolve_base(file, site);
                    (Targets::Decls(found), exact)
                }
                RefKind::References => match self.resolve_binding(file, site) {
                    Some((found, exact)) => (Targets::Decls(found), exact),
                    None => continue,
                },
                // Imports are the project pass's; dispatch is made from resolved edges, never from
                // a site here.
                RefKind::Imports | RefKind::Dispatch => continue,
            };
            let found = match targets {
                Targets::Bucket(bucket) => {
                    let dst_name = bucket.qname();
                    edges.push(Edge {
                        src: src(),
                        dst: Some(Node::Bucket(bucket)),
                        dst_name,
                        kind: site.kind,
                        exact: false,
                        line: site.line,
                    });
                    continue;
                }
                Targets::Decls(found) => found,
            };
            if found.is_empty() {
                edges.push(Edge {
                    src: src(),
                    dst: None,
                    dst_name: site_path(site),
                    kind: site.kind,
                    exact: false,
                    line: site.line,
                });
                continue;
            }
            for target in found {
                edges.push(Edge {
                    src: src(),
                    dst: Some(Node::Decl(target)),
                    dst_name: self.world.decl(target).qname.clone(),
                    kind: site.kind,
                    exact,
                    line: site.line,
                });
            }
        }
    }

    fn resolve_call(&self, file: usize, site: &RefSite) -> (Targets, bool) {
        let scope = site.scope;
        let name = site.name.as_str();
        let resolved = |found: Vec<SymRef>| {
            let (found, exact) = exact_if_one(found);
            (Targets::Decls(found), exact)
        };
        match site.qualifier.as_deref() {
            None => {
                for key in self.enclosing_types(file, scope) {
                    let members = self.find_members(&key, name);
                    if !members.is_empty() {
                        return resolved(members);
                    }
                }
                let statics = self.static_using_members(file, name);
                if !statics.is_empty() {
                    return resolved(statics);
                }
                let types = self.lookup_type(file, scope, name);
                if !types.is_empty() {
                    return resolved(self.canonicals(&types));
                }
            }
            Some("base") => {
                for key in self.enclosing_types(file, scope).into_iter().take(1) {
                    for base in self.bases.get(&key).into_iter().flatten() {
                        let members = self.find_members(base, name);
                        if !members.is_empty() {
                            return resolved(members);
                        }
                    }
                }
            }
            Some(qualifier) => {
                if let Some(key) = self.receiver_type(file, scope, site.line, qualifier) {
                    let members = self.find_members(&key, name);
                    if !members.is_empty() {
                        return resolved(members);
                    }
                    if let Some(nested) = self.nested_key(key.0, &key.1, name) {
                        return resolved(vec![self.canonical(&nested)]);
                    }
                }
                let types = self.lookup_type(file, scope, &site_path(site));
                if !types.is_empty() {
                    return resolved(self.canonicals(&types));
                }
            }
        }
        (self.fallback(name), false)
    }

    /// A base list entry resolves from outside its own type.
    fn resolve_base(&self, file: usize, site: &RefSite) -> (Vec<SymRef>, bool) {
        let context = site
            .scope
            .and_then(|scope| self.decl_at(file, scope))
            .and_then(|decl| decl.parent);
        let types = self.lookup_type(file, context, &site_path(site));
        if types.is_empty() {
            let named = self
                .types_by_name
                .get(site.name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            return (inferred(named), false);
        }
        exact_if_one(self.canonicals(&types))
    }

    /// A typed variable references its type. Types outside the repository
    /// give no edge: they would only be noise.
    fn resolve_binding(&self, file: usize, site: &RefSite) -> Option<(Vec<SymRef>, bool)> {
        let binding = parse_binding(site)?;
        let types = self.lookup_type(file, site.scope, &binding.type_path());
        if types.is_empty() {
            let named = self
                .types_by_name
                .get(binding.type_name.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let named = inferred(named);
            return (!named.is_empty()).then_some((named, false));
        }
        Some(exact_if_one(self.canonicals(&types)))
    }

    fn canonicals(&self, keys: &[Key]) -> Vec<SymRef> {
        keys.iter().map(|key| self.canonical(key)).collect()
    }

    /// Every method or field called `name`, else every type called `name`. One
    /// candidate is the target itself; several share a bucket.
    fn fallback(&self, name: &str) -> Targets {
        match self.fallback_candidates(name).as_slice() {
            [] => Targets::Decls(Vec::new()),
            [one] => Targets::Decls(vec![*one]),
            _ => Targets::Bucket(Bucket::new(WORKSPACE_SCOPE, name)),
        }
    }

    fn fallback_candidates(&self, name: &str) -> Vec<SymRef> {
        let found = self.members_by_name.get(name).or_else(|| self.types_by_name.get(name));
        found.map(Vec::as_slice).map(inferred).unwrap_or_default()
    }
}

/// What a call site resolved to: definitions, or the bucket holding them.
enum Targets {
    Decls(Vec<SymRef>),
    Bucket(Bucket),
}

/// A name-only fallback's candidates, one entry per declaration.
fn inferred(found: &[SymRef]) -> Vec<SymRef> {
    let mut found = found.to_vec();
    found.sort();
    found.dedup();
    found
}

fn exact_if_one(mut found: Vec<SymRef>) -> (Vec<SymRef>, bool) {
    found.sort();
    found.dedup();
    let exact = found.len() == 1;
    (found, exact)
}

fn site_path(site: &RefSite) -> String {
    match &site.qualifier {
        Some(qualifier) => format!("{qualifier}.{}", site.name),
        None => site.name.clone(),
    }
}

fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}
