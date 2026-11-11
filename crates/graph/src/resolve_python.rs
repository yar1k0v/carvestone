//! Python resolution. A name resolves to a top-level definition of the file,
//! then to an import binding; modules are found by their dotted path (the
//! nearest file wins when several repo files fit) and relative imports by
//! directory. `from pkg import name` finds a definition, a re-export in the
//! package's `__init__.py` or a submodule. `self.m()` and `cls.m()` in a class
//! are exact through the class and its bases; calls through an imported
//! module or class are exact. Any other attribute call `x.m()` is inferred:
//! every method named `m` (of the same project when there is one) is a target,
//! through one name bucket when there is more than one.
//!
//! Standard-library and third-party imports have no target in the repo and give
//! no edge. Not covered: `__all__`, conditional imports shadowing each other,
//! calls through instances of imported classes (these are inferred).

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{DeclKind, Lang, RefKind, RefSite};
use crate::project::ProjectKind;
use crate::resolve::{Bucket, Edge, Node, Resolver, SymRef, WORKSPACE_SCOPE, World, finish};

pub(crate) struct PythonResolver;

impl Resolver for PythonResolver {
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge> {
        let index = Index::build(world);
        let mut edges = Vec::new();
        for &file in files {
            if is_py(world, file) {
                index.resolve_file(file, &mut edges);
            }
        }
        finish(edges)
    }

    fn bucket_members(&self, world: &World, buckets: &[Bucket]) -> Vec<Vec<SymRef>> {
        let index = Index::build(world);
        let by_path: BTreeMap<&str, usize> = world
            .projects()
            .iter()
            .enumerate()
            .filter(|(_, project)| project.kind == ProjectKind::PyProject)
            .map(|(index, project)| (project.path.as_str(), index))
            .collect();
        buckets
            .iter()
            .map(|bucket| {
                let scope = by_path.get(bucket.scope.as_str()).copied();
                index.infer_candidates(scope, &bucket.name)
            })
            .collect()
    }
}

fn is_py(world: &World, file: usize) -> bool {
    world.files().get(file).is_some_and(|facts| facts.lang == Lang::Python)
}
const MAX_DEPTH: u8 = 16;

const BUILTINS: &[&str] = &[
    "print",
    "len",
    "range",
    "int",
    "str",
    "float",
    "bool",
    "list",
    "dict",
    "set",
    "tuple",
    "isinstance",
    "issubclass",
    "type",
    "super",
    "getattr",
    "setattr",
    "hasattr",
    "delattr",
    "open",
    "sorted",
    "reversed",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sum",
    "min",
    "max",
    "abs",
    "any",
    "all",
    "repr",
    "id",
    "hash",
    "iter",
    "next",
    "callable",
    "vars",
    "dir",
    "format",
    "round",
    "divmod",
    "bytes",
    "bytearray",
    "frozenset",
    "object",
    "property",
    "staticmethod",
    "classmethod",
    "input",
    "exit",
    "quit",
    "chr",
    "ord",
    "hex",
    "bin",
    "oct",
    "pow",
    "slice",
    "compile",
    "eval",
    "exec",
    "globals",
    "locals",
    "memoryview",
    "ascii",
    "complex",
    "aiter",
    "anext",
];

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

fn exact_sym(sym: SymRef) -> Found {
    Found {
        target: Target::Sym(sym),
        inferred: false,
    }
}

enum Binding<'w> {
    /// `from module import name [as local]`.
    Named { module: &'w str, name: &'w str },
    /// `import module [as local]`.
    Module { module: &'w str },
}

enum Lookup {
    Found(Vec<Found>),
    External,
    Missing,
    None,
}

enum Outcome {
    Skip,
    Unresolved,
    /// Targets, and whether one target may count as exact.
    Found(Vec<Found>, bool),
    /// Several fallback candidates behind one name bucket.
    Bucket(Bucket),
}

type Seen = BTreeSet<(usize, String)>;

struct Index<'w> {
    world: &'w World,
    tops: Vec<BTreeMap<&'w str, Vec<usize>>>,
    bindings: Vec<BTreeMap<&'w str, Binding<'w>>>,
    /// Plain `import a.b.c` strings (and their aliases) per file.
    dotted: Vec<BTreeMap<&'w str, &'w str>>,
    members: BTreeMap<SymRef, BTreeMap<&'w str, Vec<SymRef>>>,
    heritage: BTreeMap<SymRef, Vec<&'w RefSite>>,
    methods_by_name: BTreeMap<&'w str, Vec<SymRef>>,
    /// Dotted module path (every suffix of a file's path) to its files.
    modules: BTreeMap<String, Vec<usize>>,
    /// The Python project governing each file, or `None` outside every project.
    scope_of_file: Vec<Option<usize>>,
    /// Per Python project, the projects whose methods the name fallback may
    /// pick: itself and the local packages it depends on.
    visible: BTreeMap<usize, BTreeSet<usize>>,
}

impl<'w> Index<'w> {
    fn build(world: &'w World) -> Self {
        let files = world.files();
        let mut index = Index {
            world,
            tops: vec![BTreeMap::new(); files.len()],
            bindings: files.iter().map(|_| BTreeMap::new()).collect(),
            dotted: files.iter().map(|_| BTreeMap::new()).collect(),
            members: BTreeMap::new(),
            heritage: BTreeMap::new(),
            methods_by_name: BTreeMap::new(),
            modules: BTreeMap::new(),
            scope_of_file: vec![None; files.len()],
            visible: BTreeMap::new(),
        };
        for (f, facts) in files.iter().enumerate() {
            if !is_py(world, f) {
                continue;
            }
            let parts = module_parts(&facts.path);
            for start in 0..parts.len() {
                index.modules.entry(parts[start..].join(".")).or_default().push(f);
            }
            for (d, decl) in facts.decls.iter().enumerate() {
                let Some(parent) = decl.parent else { continue };
                let here = SymRef { file: f, decl: d };
                if parent == 0 {
                    if matches!(decl.kind, DeclKind::Type | DeclKind::Function | DeclKind::Member) {
                        index.tops[f].entry(decl.name.as_str()).or_default().push(d);
                    }
                } else {
                    index
                        .members
                        .entry(SymRef { file: f, decl: parent })
                        .or_default()
                        .entry(decl.name.as_str())
                        .or_default()
                        .push(here);
                }
                if decl.kind == DeclKind::Method {
                    index.methods_by_name.entry(decl.name.as_str()).or_default().push(here);
                }
            }
            for import in &facts.imports {
                if let Some((module, name)) = import.path.split_once('#') {
                    if let Some(local) = import.alias.as_deref() {
                        index.bindings[f].insert(local, Binding::Named { module, name });
                    }
                } else if !import.glob {
                    let module = import.path.as_str();
                    match import.alias.as_deref() {
                        Some(local) => {
                            index.bindings[f].insert(local, Binding::Module { module });
                        }
                        None => {
                            let first = module.split('.').next().unwrap_or(module);
                            index.bindings[f]
                                .entry(first)
                                .or_insert(Binding::Module { module: first });
                            index.dotted[f].insert(module, module);
                        }
                    }
                }
            }
            for site in &facts.refs {
                if site.kind == RefKind::Inherits
                    && let Some(scope) = site.scope
                {
                    index
                        .heritage
                        .entry(SymRef { file: f, decl: scope })
                        .or_default()
                        .push(site);
                }
            }
        }
        let mut scopes: Vec<usize> = world
            .projects()
            .iter()
            .enumerate()
            .filter(|(_, project)| project.kind == ProjectKind::PyProject)
            .map(|(p, _)| p)
            .collect();
        scopes.sort_by_key(|p| std::cmp::Reverse(world.projects()[*p].root.len()));
        for (f, facts) in files.iter().enumerate() {
            if !is_py(world, f) {
                continue;
            }
            index.scope_of_file[f] = scopes.iter().copied().find(|&p| {
                let root = &world.projects()[p].root;
                root.is_empty()
                    || facts
                        .path
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            });
        }
        let by_name: BTreeMap<&str, usize> = scopes.iter().map(|&p| (world.projects()[p].name.as_str(), p)).collect();
        let by_root: BTreeMap<&str, usize> = scopes.iter().map(|&p| (world.projects()[p].root.as_str(), p)).collect();
        for &p in &scopes {
            let project = &world.projects()[p];
            let mut visible = BTreeSet::from([p]);
            for package in &project.packages {
                if let Some(&other) = by_name.get(package.as_str()) {
                    visible.insert(other);
                }
            }
            for reference in &project.references {
                if let Some(&other) = by_root.get(reference.path.as_str()) {
                    visible.insert(other);
                }
            }
            index.visible.insert(p, visible);
        }
        index
    }

    // ------------------------------------------------------------ modules

    /// The files a module string names for the file importing it, best first
    /// (all of them when several are equally near). `None` means nothing in
    /// the repo; the flag says whether the string was relative.
    fn resolve_module(&self, file: usize, module: &str) -> (Vec<usize>, bool) {
        let from = &self.world.files()[file].path;
        let dots = module.chars().take_while(|&c| c == '.').count();
        if dots > 0 {
            let rest = &module[dots..];
            let mut base: Vec<&str> = from.split('/').collect();
            base.pop();
            for _ in 1..dots {
                if base.pop().is_none() {
                    return (Vec::new(), true);
                }
            }
            let mut path = base.join("/");
            if !rest.is_empty() {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(&rest.replace('.', "/"));
            }
            let candidates = if path.is_empty() {
                vec!["__init__.py".to_string()]
            } else if rest.is_empty() {
                vec![format!("{path}/__init__.py")]
            } else {
                vec![format!("{path}.py"), format!("{path}/__init__.py")]
            };
            let found: Vec<usize> = candidates
                .iter()
                .filter_map(|c| self.world.file_index(c))
                .filter(|&f| is_py(self.world, f))
                .take(1)
                .collect();
            return (found, true);
        }
        let Some(all) = self.modules.get(module) else {
            return (Vec::new(), false);
        };
        let near = |candidate: &usize| common_dirs(from, &self.world.files()[*candidate].path);
        let best = all.iter().map(near).max().unwrap_or(0);
        let chosen: Vec<usize> = all.iter().copied().filter(|c| near(c) == best).collect();
        (chosen, false)
    }

    fn module_attr(&self, module: usize, name: &str, seen: &mut Seen, depth: u8) -> Vec<Found> {
        if depth > MAX_DEPTH || !seen.insert((module, name.to_string())) {
            return Vec::new();
        }
        if let Some(list) = self.tops[module].get(name) {
            return list
                .iter()
                .map(|&d| exact_sym(SymRef { file: module, decl: d }))
                .collect();
        }
        if let Some(binding) = self.bindings[module].get(name)
            && let Lookup::Found(found) = self.follow(module, binding, seen, depth + 1)
        {
            return found;
        }
        // A wildcard import re-exports what it imports.
        let facts = &self.world.files()[module];
        for import in facts.imports.iter().filter(|import| import.glob) {
            let (files, _) = self.resolve_module(module, &import.path);
            for target in files {
                let found = self.module_attr(target, name, seen, depth + 1);
                if !found.is_empty() {
                    return found;
                }
            }
        }
        // A submodule of a package.
        if facts.path.ends_with("__init__.py") {
            let dir = facts.path.trim_end_matches("__init__.py").trim_end_matches('/');
            let base = if dir.is_empty() {
                name.to_string()
            } else {
                format!("{dir}/{name}")
            };
            for candidate in [format!("{base}.py"), format!("{base}/__init__.py")] {
                if let Some(f) = self.world.file_index(&candidate)
                    && is_py(self.world, f)
                {
                    return vec![Found {
                        target: Target::Module(f),
                        inferred: false,
                    }];
                }
            }
        }
        Vec::new()
    }

    fn follow(&self, file: usize, binding: &Binding, seen: &mut Seen, depth: u8) -> Lookup {
        match binding {
            Binding::Module { module } => {
                let (files, relative) = self.resolve_module(file, module);
                if files.is_empty() {
                    return if relative { Lookup::Missing } else { Lookup::External };
                }
                let several = files.len() > 1;
                Lookup::Found(
                    files
                        .into_iter()
                        .map(|f| Found {
                            target: Target::Module(f),
                            inferred: several,
                        })
                        .collect(),
                )
            }
            Binding::Named { module, name } => {
                let (files, relative) = self.resolve_module(file, module);
                let mut found: Vec<Found> = Vec::new();
                for &target in &files {
                    found.extend(self.module_attr(target, name, seen, depth));
                }
                if found.is_empty() {
                    // `from pkg import sub` where `sub` is a module.
                    let sub = if module.ends_with('.') {
                        format!("{module}{name}")
                    } else {
                        format!("{module}.{name}")
                    };
                    let (subs, _) = self.resolve_module(file, &sub);
                    found.extend(subs.into_iter().map(|f| Found {
                        target: Target::Module(f),
                        inferred: false,
                    }));
                }
                if found.is_empty() && !files.is_empty() {
                    // The module is ours but the name is not found in it.
                    found.extend(files.iter().map(|&f| Found {
                        target: Target::Module(f),
                        inferred: true,
                    }));
                }
                if found.is_empty() {
                    return if relative { Lookup::Missing } else { Lookup::External };
                }
                found.sort();
                found.dedup();
                Lookup::Found(found)
            }
        }
    }

    // ------------------------------------------------------------ names

    fn member_of(&self, sym: SymRef, name: &str, depth: u8) -> Vec<Found> {
        if let Some(list) = self.members.get(&sym).and_then(|m| m.get(name)) {
            return list.iter().copied().map(exact_sym).collect();
        }
        if depth >= 8 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for base in self.bases(sym) {
            out.extend(self.member_of(base, name, depth + 1));
        }
        out.sort();
        out.dedup();
        out
    }

    fn bases(&self, sym: SymRef) -> Vec<SymRef> {
        let mut out = Vec::new();
        for site in self.heritage.get(&sym).into_iter().flatten() {
            if let Lookup::Found(found) = self.lookup_path(sym.file, &site_segments(site)) {
                for hit in found {
                    if let Target::Sym(base) = hit.target
                        && base != sym
                        && self.world.decl(base).kind == DeclKind::Type
                    {
                        out.push(base);
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn lookup_path(&self, file: usize, segments: &[&str]) -> Lookup {
        if segments.is_empty() {
            return Lookup::None;
        }
        let mut start = 1;
        let mut current: Vec<Found>;
        // `import a.b.c` then `a.b.c.f()`: the longest imported dotted prefix.
        let dotted_hit = (2..segments.len()).rev().find_map(|k| {
            let prefix = segments[..k].join(".");
            self.dotted[file].get(prefix.as_str()).map(|module| (k, *module))
        });
        if let Some((k, module)) = dotted_hit {
            match self.follow(file, &Binding::Module { module }, &mut Seen::new(), 0) {
                Lookup::Found(found) => {
                    current = found;
                    start = k;
                }
                other => return other,
            }
        } else if let Some(list) = self.tops[file].get(segments[0]) {
            current = list.iter().map(|&d| exact_sym(SymRef { file, decl: d })).collect();
        } else if let Some(binding) = self.bindings[file].get(segments[0]) {
            match self.follow(file, binding, &mut Seen::new(), 0) {
                Lookup::Found(found) => current = found,
                other => return other,
            }
        } else {
            // `from m import *` brings in whatever `m` defines.
            let mut wild: Vec<Found> = Vec::new();
            for import in self.world.files()[file].imports.iter().filter(|import| import.glob) {
                let (files, _) = self.resolve_module(file, &import.path);
                for target in files {
                    wild.extend(self.module_attr(target, segments[0], &mut Seen::new(), 0));
                }
            }
            if wild.is_empty() {
                return Lookup::None;
            }
            wild.sort();
            wild.dedup();
            current = wild;
        }
        for segment in &segments[start..] {
            let mut next: Vec<Found> = Vec::new();
            for hit in &current {
                let found = match hit.target {
                    Target::Module(module) => self.module_attr(module, segment, &mut Seen::new(), 0),
                    Target::Sym(sym) => self.member_of(sym, segment, 0),
                };
                next.extend(found.into_iter().map(|f| Found {
                    target: f.target,
                    inferred: f.inferred || hit.inferred,
                }));
            }
            if next.is_empty() {
                return Lookup::None;
            }
            next.sort();
            next.dedup();
            current = next;
        }
        Lookup::Found(current)
    }

    fn sym_of(target: Target) -> SymRef {
        match target {
            Target::Module(file) => SymRef { file, decl: 0 },
            Target::Sym(sym) => sym,
        }
    }

    // ------------------------------------------------------------ sites

    fn resolve_file(&self, file: usize, edges: &mut Vec<Edge>) {
        let world = self.world;
        for site in &world.files()[file].refs {
            let src = || {
                Node::Decl(SymRef {
                    file,
                    decl: site.scope.unwrap_or(0),
                })
            };
            let outcome = match site.kind {
                RefKind::Imports => self.resolve_import(file, site),
                RefKind::Calls => self.resolve_call(file, site),
                RefKind::Inherits | RefKind::Implements => match self.lookup_path(file, &site_segments(site)) {
                    Lookup::Found(found) => Outcome::Found(found, true),
                    Lookup::External => Outcome::Skip,
                    Lookup::Missing | Lookup::None => {
                        if site.qualifier.is_none() && is_builtin_like(&site.name) {
                            Outcome::Skip
                        } else {
                            Outcome::Unresolved
                        }
                    }
                },
                RefKind::References => match self.lookup_path(file, &site_segments(site)) {
                    Lookup::Found(found) => Outcome::Found(found, true),
                    _ => Outcome::Skip,
                },
                // Only the dispatch pass makes this, from resolved edges.
                RefKind::Dispatch => Outcome::Skip,
            };
            match outcome {
                Outcome::Skip => {}
                Outcome::Unresolved => edges.push(Edge {
                    src: src(),
                    dst: None,
                    dst_name: written(site),
                    kind: site.kind,
                    exact: false,
                    line: site.line,
                }),
                Outcome::Found(found, may_be_exact) => {
                    let single = found.len() == 1;
                    for hit in found {
                        let target = Self::sym_of(hit.target);
                        edges.push(Edge {
                            src: src(),
                            dst: Some(Node::Decl(target)),
                            dst_name: world.decl(target).qname.clone(),
                            kind: site.kind,
                            exact: may_be_exact && single && !hit.inferred,
                            line: site.line,
                        });
                    }
                }
                Outcome::Bucket(bucket) => {
                    let dst_name = bucket.qname();
                    edges.push(Edge {
                        src: src(),
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

    fn resolve_import(&self, file: usize, site: &RefSite) -> Outcome {
        let module = site.qualifier.as_deref().unwrap_or("");
        let looked = if site.name == "*" {
            self.follow(file, &Binding::Module { module }, &mut Seen::new(), 0)
        } else {
            self.follow(
                file,
                &Binding::Named {
                    module,
                    name: &site.name,
                },
                &mut Seen::new(),
                0,
            )
        };
        match looked {
            Lookup::Found(found) => Outcome::Found(found, true),
            Lookup::External => Outcome::Skip,
            Lookup::Missing | Lookup::None => Outcome::Unresolved,
        }
    }

    fn resolve_call(&self, file: usize, site: &RefSite) -> Outcome {
        let Some(qualifier) = site.qualifier.as_deref() else {
            return match self.lookup_path(file, &[site.name.as_str()]) {
                Lookup::Found(found) => Outcome::Found(found, true),
                Lookup::External => Outcome::Skip,
                Lookup::Missing | Lookup::None => {
                    if BUILTINS.contains(&site.name.as_str()) || is_builtin_like(&site.name) {
                        Outcome::Skip
                    } else {
                        Outcome::Unresolved
                    }
                }
            };
        };
        let receiver = qualifier.strip_prefix('.').unwrap_or(qualifier);
        if matches!(receiver, "self" | "cls" | "super()") {
            if let Some(class) = self.enclosing_type(file, site.scope) {
                let found = if receiver == "super()" {
                    let mut found = Vec::new();
                    for base in self.bases(class) {
                        found.extend(self.member_of(base, &site.name, 0));
                    }
                    found
                } else {
                    self.member_of(class, &site.name, 0)
                };
                if !found.is_empty() {
                    return Outcome::Found(found, true);
                }
            }
            return self.infer_method(file, &site.name);
        }
        if let Some(mut segments) = path_segments(receiver) {
            segments.push(site.name.as_str());
            match self.lookup_path(file, &segments) {
                Lookup::Found(found) => return Outcome::Found(found, true),
                Lookup::External => return Outcome::Skip,
                Lookup::Missing => return Outcome::Unresolved,
                Lookup::None => {}
            }
        }
        self.infer_method(file, &site.name)
    }

    fn enclosing_type(&self, file: usize, scope: Option<usize>) -> Option<SymRef> {
        let decls = &self.world.files()[file].decls;
        let mut current = scope?;
        loop {
            let decl = decls.get(current)?;
            if decl.kind == DeclKind::Type {
                return Some(SymRef { file, decl: current });
            }
            current = decl.parent?;
        }
    }

    fn scope_of(&self, file: usize) -> Option<usize> {
        self.scope_of_file.get(file).copied().flatten()
    }

    /// Whether a fallback for code in project `from` may pick project `to`.
    fn sees(&self, from: usize, to: usize) -> bool {
        from == to || self.visible.get(&from).is_some_and(|set| set.contains(&to))
    }

    /// The name-only fallback is scoped like Rust's: methods of the file's own
    /// project, else of the projects it depends on. A file outside every
    /// project still sees the whole workspace.
    fn infer_method(&self, file: usize, name: &str) -> Outcome {
        let chosen = self.infer_candidates(self.scope_of(file), name);
        match chosen.as_slice() {
            [] => Outcome::Unresolved,
            [_one] => Outcome::Found(chosen.into_iter().map(exact_sym).collect(), false),
            _ => {
                let scope = self.scope_of(file).map_or_else(
                    || WORKSPACE_SCOPE.to_string(),
                    |p| self.world.projects()[p].path.clone(),
                );
                Outcome::Bucket(Bucket::new(scope, name))
            }
        }
    }

    fn infer_candidates(&self, scope: Option<usize>, name: &str) -> Vec<SymRef> {
        let Some(all) = self.methods_by_name.get(name) else {
            return Vec::new();
        };
        match scope {
            Some(scope) => all
                .iter()
                .copied()
                .filter(|sym| self.scope_of(sym.file).is_some_and(|other| self.sees(scope, other)))
                .collect(),
            None => all.clone(),
        }
    }
}

// ---------------------------------------------------------------- helpers

/// Capitalised names that no import explains are standard exceptions and
/// classes far more often than repo types.
fn is_builtin_like(name: &str) -> bool {
    name.starts_with(|c: char| c.is_uppercase())
}

/// The path segments of a site: its dotted qualifier then its name.
fn site_segments(site: &RefSite) -> Vec<&str> {
    let mut segments: Vec<&str> = match site.qualifier.as_deref() {
        Some(q) if !q.starts_with('.') => q.split('.').filter(|part| !part.is_empty()).collect(),
        _ => Vec::new(),
    };
    segments.push(site.name.as_str());
    segments
}

/// `a.b.c` as segments, or `None` when the receiver is an expression.
fn path_segments(receiver: &str) -> Option<Vec<&str>> {
    let segments: Vec<&str> = receiver.split('.').collect();
    let is_name = |s: &&str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    if segments.iter().all(is_name) {
        Some(segments)
    } else {
        None
    }
}

/// The target as written, for an edge that found nothing.
fn written(site: &RefSite) -> String {
    match site.kind {
        RefKind::Imports => site.qualifier.clone().unwrap_or_else(|| site.name.clone()),
        _ => match site.qualifier.as_deref() {
            Some(q) => {
                let q = q.strip_prefix('.').unwrap_or(q);
                if q.is_empty() {
                    site.name.clone()
                } else {
                    format!("{q}.{}", site.name)
                }
            }
            None => site.name.clone(),
        },
    }
}

/// The dotted-module parts of a file path: directories then the stem, with
/// `__init__` naming its package.
fn module_parts(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if let Some(last) = parts.pop() {
        let stem = last.strip_suffix(".py").unwrap_or(last);
        if stem != "__init__" {
            parts.push(stem);
        }
    }
    parts
}

/// How many leading directories two file paths share.
fn common_dirs(a: &str, b: &str) -> usize {
    let da: Vec<&str> = a.split('/').collect();
    let db: Vec<&str> = b.split('/').collect();
    let (da, db) = (&da[..da.len() - 1], &db[..db.len() - 1]);
    da.iter().zip(db.iter()).take_while(|(x, y)| x == y).count()
}
