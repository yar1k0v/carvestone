//! TypeScript and JavaScript resolution. A name resolves to a local top-level
//! declaration first, then to an import binding; relative specifiers, `paths`
//! and `baseUrl` of the governing `tsconfig.json` find the target file, and its
//! exports (including `export ... from` chains and `export *`) find the
//! declaration. Calls and types through a binding, and `this.f()` inside a
//! class, are exact when they end on one definition. A bare package import
//! (`react`, `lodash`) has no target in the repo and gives no edge. Any other
//! member call `x.f()` is inferred: every method named `f` (of the same
//! project when there is one) is a target, through one name bucket when there
//! is more than one.
//!
//! Not covered: `tsconfig` `extends`, package.json `exports` maps, workspace
//! package names, CommonJS `module.exports` / `exports.x =` assignments.

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{DeclKind, Lang, RefKind, RefSite};
use crate::project::ProjectKind;
use crate::resolve::{Bucket, Edge, Node, Resolver, SymRef, WORKSPACE_SCOPE, World, finish};

pub(crate) struct TsResolver;

impl Resolver for TsResolver {
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge> {
        let index = Index::build(world);
        let mut edges = Vec::new();
        for &file in files {
            if is_ts(world, file) {
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

    fn resolve_projects(&self, world: &World) -> Vec<Edge> {
        let mut edges = Vec::new();
        for (index, project) in world.projects().iter().enumerate() {
            if project.kind != ProjectKind::TsConfig {
                continue;
            }
            for reference in &project.references {
                let dst = world.projects().iter().position(|other| {
                    other.kind == ProjectKind::TsConfig
                        && (other.path == reference.path
                            || other.path == format!("{}/tsconfig.json", reference.path.trim_end_matches('/'))
                            || (other.root == reference.path && other.path.ends_with("tsconfig.json")))
                });
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

fn is_ts(world: &World, file: usize) -> bool {
    world
        .files()
        .get(file)
        .is_some_and(|facts| matches!(facts.lang, Lang::TypeScript | Lang::JavaScript))
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

fn exact_sym(sym: SymRef) -> Found {
    Found {
        target: Target::Sym(sym),
        inferred: false,
    }
}

/// Where a specifier points.
enum Spec {
    /// A bare package: nothing in the repo.
    External,
    /// A repo-local specifier whose file is not there.
    Missing,
    File(usize),
}

enum Binding<'w> {
    Named { spec: &'w str, name: &'w str },
    Namespace { spec: &'w str },
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
    /// Per file, its top-level declarations by name.
    tops: Vec<BTreeMap<&'w str, Vec<usize>>>,
    /// Per file, its import bindings by local name.
    bindings: Vec<BTreeMap<&'w str, Binding<'w>>>,
    /// Members of classes, interfaces, enums and namespaces by name.
    members: BTreeMap<SymRef, BTreeMap<&'w str, Vec<SymRef>>>,
    /// The `extends`/`implements` sites of each class or interface.
    heritage: BTreeMap<SymRef, Vec<&'w RefSite>>,
    methods_by_name: BTreeMap<&'w str, Vec<SymRef>>,
    /// Indices of the tsconfig projects, deepest directory first.
    tsconfigs: Vec<usize>,
    /// Indices of the tsconfig and package.json projects, deepest first.
    scopes: Vec<usize>,
}

impl<'w> Index<'w> {
    fn build(world: &'w World) -> Self {
        let files = world.files();
        let mut index = Index {
            world,
            tops: vec![BTreeMap::new(); files.len()],
            bindings: files.iter().map(|_| BTreeMap::new()).collect(),
            members: BTreeMap::new(),
            heritage: BTreeMap::new(),
            methods_by_name: BTreeMap::new(),
            tsconfigs: Vec::new(),
            scopes: Vec::new(),
        };
        for (f, facts) in files.iter().enumerate() {
            if !is_ts(world, f) {
                continue;
            }
            for (d, decl) in facts.decls.iter().enumerate() {
                let Some(parent) = decl.parent else { continue };
                let here = SymRef { file: f, decl: d };
                if parent == 0 {
                    if matches!(
                        decl.kind,
                        DeclKind::Type | DeclKind::Function | DeclKind::Member | DeclKind::Module
                    ) {
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
                if import.is_static {
                    continue;
                }
                let Some(local) = import.alias.as_deref() else {
                    continue;
                };
                let binding = if import.glob {
                    Binding::Namespace { spec: &import.path }
                } else if let Some((spec, name)) = import.path.rsplit_once('#') {
                    Binding::Named { spec, name }
                } else {
                    continue;
                };
                index.bindings[f].insert(local, binding);
            }
            for site in &facts.refs {
                if matches!(site.kind, RefKind::Inherits | RefKind::Implements)
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
        let mut tsconfigs: Vec<usize> = Vec::new();
        let mut scopes: Vec<usize> = Vec::new();
        for (p, project) in world.projects().iter().enumerate() {
            match project.kind {
                ProjectKind::TsConfig => {
                    tsconfigs.push(p);
                    scopes.push(p);
                }
                ProjectKind::Npm => scopes.push(p),
                _ => {}
            }
        }
        let depth = |p: &usize| std::cmp::Reverse(world.projects()[*p].root.len());
        tsconfigs.sort_by_key(depth);
        scopes.sort_by_key(depth);
        index.tsconfigs = tsconfigs;
        index.scopes = scopes;
        index
    }

    // ------------------------------------------------------------ specifiers

    fn find_file(&self, path: &str) -> Option<usize> {
        let path = normalize(path)?;
        let is_code = |index: usize| is_ts(self.world, index);
        let mut candidates: Vec<String> = Vec::new();
        if let Some((stem, ext)) = path.rsplit_once('.')
            && matches!(ext, "js" | "jsx" | "mjs" | "cjs")
            && !stem.ends_with('/')
        {
            // ESM sources written for the compiler name `.js` and mean `.ts`.
            candidates.push(format!("{stem}.ts"));
            candidates.push(format!("{stem}.tsx"));
        }
        candidates.push(path.clone());
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs"] {
            candidates.push(format!("{path}.{ext}"));
        }
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs"] {
            candidates.push(format!("{path}/index.{ext}"));
        }
        candidates
            .iter()
            .filter_map(|candidate| self.world.file_index(candidate))
            .find(|&index| is_code(index))
    }

    fn resolve_spec(&self, file: usize, spec: &str) -> Spec {
        let from = &self.world.files()[file].path;
        if spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../") {
            let dir = dir_of(from);
            return match self.find_file(&join(dir, spec)) {
                Some(found) => Spec::File(found),
                None => Spec::Missing,
            };
        }
        for &p in &self.tsconfigs {
            let project = &self.world.projects()[p];
            if !contains(&project.root, from) {
                continue;
            }
            let mut hits: Vec<(usize, &str, &str)> = project
                .aliases
                .iter()
                .filter_map(|alias| {
                    capture(&alias.pattern, spec).map(|rest| {
                        (
                            alias.pattern.find('*').unwrap_or(usize::MAX),
                            rest,
                            alias.target.as_str(),
                        )
                    })
                })
                .collect();
            // The pattern with the longest literal prefix wins; ties keep file order.
            hits.sort_by_key(|(prefix, _, _)| std::cmp::Reverse(*prefix));
            // A catch-all `*` that finds nothing falls through to node_modules.
            let matched = hits.iter().any(|(prefix, _, _)| *prefix != 0);
            for (_, rest, target) in hits {
                if let Some(found) = self.find_file(&target.replacen('*', rest, 1)) {
                    return Spec::File(found);
                }
            }
            if let Some(base) = &project.base_url
                && let Some(found) = self.find_file(&join(base, spec))
            {
                return Spec::File(found);
            }
            if matched {
                return Spec::Missing;
            }
        }
        Spec::External
    }

    /// The deepest tsconfig or package.json directory holding `file`.
    fn scope_of(&self, file: usize) -> Option<usize> {
        let path = &self.world.files()[file].path;
        self.scopes
            .iter()
            .copied()
            .find(|&p| contains(&self.world.projects()[p].root, path))
    }

    // ------------------------------------------------------------ exports

    fn exports(&self, file: usize, name: &str, seen: &mut Seen, depth: u8) -> Vec<Found> {
        if depth > MAX_DEPTH || !seen.insert((file, name.to_string())) {
            return Vec::new();
        }
        let facts = &self.world.files()[file];
        let mut out: Vec<Found> = Vec::new();
        for (d, decl) in facts.decls.iter().enumerate() {
            if decl.parent != Some(0) {
                continue;
            }
            let exported_as = if decl.signature.starts_with("export default ") {
                "default"
            } else if decl.signature.starts_with("export ") {
                decl.name.as_str()
            } else {
                continue;
            };
            if exported_as == name {
                out.push(exact_sym(SymRef { file, decl: d }));
            }
        }
        for import in facts.imports.iter().filter(|import| import.is_static) {
            match (import.glob, import.alias.as_deref()) {
                (false, Some(as_name)) if as_name == name => {
                    let (spec, orig) = import.path.rsplit_once('#').unwrap_or((import.path.as_str(), ""));
                    if spec.is_empty() {
                        if let Some(list) = self.tops[file].get(orig) {
                            out.extend(list.iter().map(|&d| exact_sym(SymRef { file, decl: d })));
                        } else if let Some(binding) = self.bindings[file].get(orig)
                            && let Lookup::Found(found) = self.follow(file, binding, seen, depth + 1)
                        {
                            out.extend(found);
                        }
                    } else if let Spec::File(target) = self.resolve_spec(file, spec) {
                        out.extend(self.exports(target, orig, seen, depth + 1));
                    }
                }
                (true, Some(namespace)) if namespace == name => {
                    if let Spec::File(target) = self.resolve_spec(file, &import.path) {
                        out.push(Found {
                            target: Target::Module(target),
                            inferred: false,
                        });
                    }
                }
                _ => {}
            }
        }
        if out.is_empty() && name != "default" {
            for import in facts
                .imports
                .iter()
                .filter(|import| import.is_static && import.glob && import.alias.is_none())
            {
                if let Spec::File(target) = self.resolve_spec(file, &import.path) {
                    out.extend(self.exports(target, name, seen, depth + 1));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// What an import binding stands for.
    fn follow(&self, file: usize, binding: &Binding, seen: &mut Seen, depth: u8) -> Lookup {
        match binding {
            Binding::Namespace { spec } => match self.resolve_spec(file, spec) {
                Spec::File(target) => Lookup::Found(vec![Found {
                    target: Target::Module(target),
                    inferred: false,
                }]),
                Spec::Missing => Lookup::Missing,
                Spec::External => Lookup::External,
            },
            Binding::Named { spec, name } => match self.resolve_spec(file, spec) {
                Spec::File(target) => {
                    let found = self.exports(target, name, seen, depth);
                    if !found.is_empty() {
                        return Lookup::Found(found);
                    }
                    // CommonJS or an export form we do not read: any top-level
                    // declaration of that name, else the module itself.
                    let fallback: Vec<Found> = self.tops[target]
                        .get(name)
                        .into_iter()
                        .flatten()
                        .map(|&d| Found {
                            target: Target::Sym(SymRef { file: target, decl: d }),
                            inferred: true,
                        })
                        .collect();
                    if fallback.is_empty() {
                        Lookup::Found(vec![Found {
                            target: Target::Module(target),
                            inferred: true,
                        }])
                    } else {
                        Lookup::Found(fallback)
                    }
                }
                Spec::Missing => Lookup::Missing,
                Spec::External => Lookup::External,
            },
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
            let segments = site_segments(site);
            if let Lookup::Found(found) = self.lookup_path(sym.file, &segments) {
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
        let Some(first) = segments.first() else {
            return Lookup::None;
        };
        let mut current: Vec<Found> = if let Some(list) = self.tops[file].get(first) {
            list.iter().map(|&d| exact_sym(SymRef { file, decl: d })).collect()
        } else if let Some(binding) = self.bindings[file].get(first) {
            match self.follow(file, binding, &mut Seen::new(), 0) {
                Lookup::Found(found) => found,
                other => return other,
            }
        } else {
            return Lookup::None;
        };
        for segment in &segments[1..] {
            let mut next: Vec<Found> = Vec::new();
            for hit in &current {
                let found = match hit.target {
                    Target::Module(module) => {
                        let exported = self.exports(module, segment, &mut Seen::new(), 0);
                        if exported.is_empty() {
                            // CommonJS: what the module assigns to `exports` is not read.
                            self.tops[module]
                                .get(segment)
                                .into_iter()
                                .flatten()
                                .map(|&d| Found {
                                    target: Target::Sym(SymRef { file: module, decl: d }),
                                    inferred: true,
                                })
                                .collect()
                        } else {
                            exported
                        }
                    }
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
                    Lookup::Missing | Lookup::None => Outcome::Unresolved,
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
        let spec = site.qualifier.as_deref().unwrap_or("");
        match self.resolve_spec(file, spec) {
            Spec::External => Outcome::Skip,
            Spec::Missing => Outcome::Unresolved,
            Spec::File(target) => {
                if site.name == "*" {
                    return Outcome::Found(
                        vec![Found {
                            target: Target::Module(target),
                            inferred: false,
                        }],
                        true,
                    );
                }
                let found = self.exports(target, &site.name, &mut Seen::new(), 0);
                if !found.is_empty() {
                    return Outcome::Found(found, true);
                }
                let fallback: Vec<Found> = self.tops[target]
                    .get(site.name.as_str())
                    .into_iter()
                    .flatten()
                    .map(|&d| Found {
                        target: Target::Sym(SymRef { file: target, decl: d }),
                        inferred: true,
                    })
                    .collect();
                if fallback.is_empty() {
                    Outcome::Found(
                        vec![Found {
                            target: Target::Module(target),
                            inferred: true,
                        }],
                        true,
                    )
                } else {
                    Outcome::Found(fallback, true)
                }
            }
        }
    }

    fn resolve_call(&self, file: usize, site: &RefSite) -> Outcome {
        let Some(qualifier) = site.qualifier.as_deref() else {
            return match self.lookup_path(file, &[site.name.as_str()]) {
                Lookup::Found(found) => Outcome::Found(found, true),
                Lookup::External => Outcome::Skip,
                Lookup::Missing | Lookup::None => Outcome::Unresolved,
            };
        };
        let receiver = qualifier.strip_prefix('.').unwrap_or(qualifier);
        if receiver == "this" || receiver == "super" {
            if let Some(class) = self.enclosing_type(file, site.scope) {
                let found = if receiver == "this" {
                    self.member_of(class, &site.name, 0)
                } else {
                    let mut found = Vec::new();
                    for base in self.bases(class) {
                        found.extend(self.member_of(base, &site.name, 0));
                    }
                    found
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
        let same: Vec<SymRef> = all
            .iter()
            .copied()
            .filter(|sym| scope.is_some() && self.scope_of(sym.file) == scope)
            .collect();
        if same.is_empty() { all.clone() } else { same }
    }
}

// ---------------------------------------------------------------- helpers

/// The path segments of a site: its dotted qualifier then its name.
fn site_segments(site: &RefSite) -> Vec<&str> {
    let mut segments: Vec<&str> = match site.qualifier.as_deref() {
        Some(q) if !q.starts_with(['.', '!']) => q.split('.').filter(|part| !part.is_empty()).collect(),
        _ => Vec::new(),
    };
    segments.push(site.name.as_str());
    segments
}

/// `a.b.c` as segments, or `None` when the receiver is an expression.
fn path_segments(receiver: &str) -> Option<Vec<&str>> {
    let segments: Vec<&str> = receiver.split('.').collect();
    let is_name = |s: &&str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
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

/// What `*` matched when `spec` fits `pattern`; the whole spec for a pattern
/// without a star that equals it.
fn capture<'s>(pattern: &str, spec: &'s str) -> Option<&'s str> {
    match pattern.split_once('*') {
        None => (pattern == spec).then_some(""),
        Some((prefix, suffix)) => {
            let rest = spec.strip_prefix(prefix)?.strip_suffix(suffix)?;
            Some(rest)
        }
    }
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

/// Resolve `.` and `..` lexically; `None` when the path walks above the repo.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Whether `path` lies in directory `root` (empty is the repo root).
fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}
