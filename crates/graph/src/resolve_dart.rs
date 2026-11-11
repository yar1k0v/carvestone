//! Dart resolution. Directives first: `'a/b.dart'` is relative to the importing file's folder (and
//! never leaves the project it sits in), `package:<name>/x.dart` is `lib/x.dart` of the pubspec
//! named `<name>`, looked up in the importing file's own project first (two apps may share a
//! name, so a twin is never reached from here), then in the projects it references by path, then
//! in the other projects of the world by name (the nearest one when several share it). `dart:`
//! and every package without a project in the repo give no edge, and neither does an import whose
//! file is not in the index (a generated `.g.dart`, a missing file): none of these is an error.
//!
//! A library is a file plus the files that say `part of` it; its parts share one scope and the
//! imports of the library file. A name is looked up in this order: the members of the enclosing
//! type and its bases (an implicit `this`), the declarations of the library, then what the
//! library's imports bring in. An import brings the public top-level names of its target and of
//! everything the target `export`s (cycles and chains followed, `show` and `hide` applied at every
//! step); with `as p` the names are only reachable as `p.Name`. Names starting with `_` stay in
//! their library.
//!
//! Sites:
//! - `extends`, `with`, `implements`, `on` resolve to a type (exact when it is one);
//! - a plain call `f()` or `Foo()` resolves to a member, a top-level function or a type (a
//!   constructor call links the type); a member call `a.b.c()` walks its receiver: `this`,
//!   `super`, an import prefix, a type name (static members, named constructors), a parameter, a
//!   field or a top-level variable whose declared type is a repo type. A receiver typed with a type
//!   outside the repo (`Dio`, `WidgetRef`, `BuildContext`) gives no edge, so the Flutter and
//!   package call noise stays out;
//! - a receiver whose type is unknown (a local variable, a call result) falls back to the name:
//!   every method of that name in the same project (or the projects it references), one inferred
//!   edge when there is one, a name bucket when there are several. A few names every Dart type
//!   defines (`add`, `map`, `toString`, ...) and private names of other libraries never take
//!   part;
//! - `Foo.values`, `Foo.of(...)` and the like, where the type is a repo type and has no such
//!   member, link the type (inferred);
//! - a type written anywhere, an annotation, a type-like receiver and a bare name given to a call
//!   are references to the declaration they name.
//!
//! Dart resolves nothing across a project folder by name only. Not covered: conditional imports
//! other than the first URI, `library` names, extension methods (found only by the name
//! fallback), types of local variables and call results.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::facts::{DeclKind, Lang, RefKind, RefSite};
use crate::project::ProjectKind;
use crate::resolve::{Bucket, Edge, Node, Resolver, SymRef, WORKSPACE_SCOPE, World, finish};

pub(crate) struct DartResolver;

impl Resolver for DartResolver {
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge> {
        let index = Index::build(world);
        let mut edges = Vec::new();
        for &file in files {
            if is_dart(world, file) {
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
            .filter(|(_, project)| project.kind == ProjectKind::Pubspec)
            .map(|(index, project)| (project.path.as_str(), index))
            .collect();
        buckets
            .iter()
            .map(|bucket| index.infer_candidates(by_path.get(bucket.scope.as_str()).copied(), &bucket.name))
            .collect()
    }
}

fn is_dart(world: &World, file: usize) -> bool {
    world.files().get(file).is_some_and(|facts| facts.lang == Lang::Dart)
}

const MAX_DEPTH: u8 = 8;

/// Names that nearly every Dart type or Flutter class defines: a guess by name would link the
/// wrong thing far more often than the right one.
const COMMON: &[&str] = &[
    "toString",
    "hashCode",
    "runtimeType",
    "noSuchMethod",
    "add",
    "addAll",
    "remove",
    "removeAt",
    "removeWhere",
    "clear",
    "map",
    "where",
    "toList",
    "toSet",
    "forEach",
    "contains",
    "containsKey",
    "join",
    "split",
    "trim",
    "call",
    "of",
    "then",
    "catchError",
    "whenComplete",
    "listen",
    "cancel",
    "close",
    "dispose",
    "build",
    "initState",
    "setState",
    "notifyListeners",
    "copyWith",
    "toJson",
    "fromJson",
    "length",
    "first",
    "last",
    "name",
];

/// Words that come before a declared type and are not part of it.
const MODIFIERS: &[&str] = &[
    "final",
    "late",
    "static",
    "const",
    "var",
    "external",
    "covariant",
    "required",
    "abstract",
    "factory",
];

type Names<'w> = BTreeMap<&'w str, Vec<SymRef>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Import,
    Export,
    Part,
    PartOf,
}

struct Directive<'w> {
    kind: Dir,
    uri: &'w str,
    prefix: Option<&'w str>,
    show: Option<Vec<&'w str>>,
    hide: Vec<&'w str>,
    line: u32,
    target: Option<usize>,
}

impl Directive<'_> {
    fn allows(&self, name: &str) -> bool {
        self.show.as_ref().is_none_or(|list| list.contains(&name)) && !self.hide.contains(&name)
    }

    /// `part of 'file.dart'` names a file; `part of lib.name;` names a library.
    fn names_file(&self) -> bool {
        self.uri.ends_with(".dart") || self.uri.contains('/')
    }
}

fn parse_directive(site: &RefSite) -> Option<Directive<'_>> {
    let words = site.qualifier.as_deref()?;
    let mut parts = words.split(' ');
    let kind = match parts.next()? {
        "import" => Dir::Import,
        "export" => Dir::Export,
        "part" => Dir::Part,
        "part-of" => Dir::PartOf,
        _ => return None,
    };
    let mut directive = Directive {
        kind,
        uri: site.name.as_str(),
        prefix: None,
        show: None,
        hide: Vec::new(),
        line: site.line,
        target: None,
    };
    for word in parts {
        if let Some(prefix) = word.strip_prefix("as=") {
            directive.prefix = Some(prefix);
        } else if let Some(list) = word.strip_prefix("show=") {
            directive
                .show
                .get_or_insert_with(Vec::new)
                .extend(list.split(',').filter(|name| !name.is_empty()));
        } else if let Some(list) = word.strip_prefix("hide=") {
            directive.hide.extend(list.split(',').filter(|name| !name.is_empty()));
        }
    }
    Some(directive)
}

/// What a resolved site becomes.
enum Outcome {
    Skip,
    /// Targets, and whether one target may count as exact.
    Found(Vec<SymRef>, bool),
    /// Several fallback candidates behind one name bucket.
    Bucket(Bucket),
}

/// A declared type: the import prefix it was written with, and its name.
#[derive(Clone, Copy)]
struct TypeRef<'a> {
    prefix: Option<&'a str>,
    name: &'a str,
}

/// Where a path walk stands after some segments.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cur<'s> {
    Type(SymRef),
    Ns(&'s str),
}

enum Walked {
    Found(Vec<SymRef>),
    /// The receiver is a repo type that has no such member. `via_type`: it was written as a type
    /// name (`Foo.values`), not reached through a value.
    Missing {
        types: Vec<SymRef>,
        via_type: bool,
    },
    /// The path leaves the repo: an external type or package.
    External,
    /// The path is a value of unknown type.
    Unknown,
}

struct Index<'w> {
    world: &'w World,
    /// Per file, the top-level declarations by name (types, functions, variables).
    tops: Vec<BTreeMap<&'w str, Vec<usize>>>,
    members: BTreeMap<SymRef, BTreeMap<&'w str, Vec<SymRef>>>,
    heritage: BTreeMap<SymRef, Vec<&'w RefSite>>,
    methods_by_name: BTreeMap<&'w str, Vec<SymRef>>,
    /// The pubspec project governing each file.
    scope_of_file: Vec<Option<usize>>,
    /// Per pubspec project, itself and the projects it references by path.
    visible: BTreeMap<usize, BTreeSet<usize>>,
    /// Pubspec name to its projects (several when two apps share a name).
    packages: BTreeMap<&'w str, Vec<usize>>,
    directives: Vec<Vec<Directive<'w>>>,
    /// The file that holds each file's library: itself, or the file it is a `part of`.
    lib_of: Vec<usize>,
    lib_files: BTreeMap<usize, Vec<usize>>,
    exported: RefCell<BTreeMap<usize, Rc<Names<'w>>>>,
}

impl<'w> Index<'w> {
    fn build(world: &'w World) -> Self {
        let files = world.files();
        let mut index = Index {
            world,
            tops: vec![BTreeMap::new(); files.len()],
            members: BTreeMap::new(),
            heritage: BTreeMap::new(),
            methods_by_name: BTreeMap::new(),
            scope_of_file: vec![None; files.len()],
            visible: BTreeMap::new(),
            packages: BTreeMap::new(),
            directives: files.iter().map(|_| Vec::new()).collect(),
            lib_of: (0..files.len()).collect(),
            lib_files: BTreeMap::new(),
            exported: RefCell::new(BTreeMap::new()),
        };
        for (f, facts) in files.iter().enumerate() {
            if !is_dart(world, f) {
                continue;
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
                if decl.kind == DeclKind::Method && decl.name != "new" && !decl.name.starts_with("operator") {
                    index.methods_by_name.entry(decl.name.as_str()).or_default().push(here);
                }
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

        // Pubspec projects: the one governing a file is the deepest whose folder holds it.
        let mut scopes: Vec<usize> = world
            .projects()
            .iter()
            .enumerate()
            .filter(|(_, project)| project.kind == ProjectKind::Pubspec)
            .map(|(p, _)| p)
            .collect();
        scopes.sort_by_key(|p| std::cmp::Reverse(world.projects()[*p].root.len()));
        for (f, facts) in files.iter().enumerate() {
            if !is_dart(world, f) {
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
        let by_root: BTreeMap<&str, usize> = scopes.iter().map(|&p| (world.projects()[p].root.as_str(), p)).collect();
        for &p in &scopes {
            let project = &world.projects()[p];
            index.packages.entry(project.name.as_str()).or_default().push(p);
            let mut visible = BTreeSet::from([p]);
            for reference in &project.references {
                if let Some(&other) = by_root.get(reference.path.as_str()) {
                    visible.insert(other);
                }
            }
            index.visible.insert(p, visible);
        }

        // Directives, then their targets (which need the project lookups above).
        let mut parsed: Vec<Vec<Directive<'w>>> = files.iter().map(|_| Vec::new()).collect();
        for (f, facts) in files.iter().enumerate() {
            if is_dart(world, f) {
                parsed[f] = facts
                    .refs
                    .iter()
                    .filter(|site| site.kind == RefKind::Imports)
                    .filter_map(parse_directive)
                    .collect();
            }
        }
        for (f, list) in parsed.iter_mut().enumerate() {
            for directive in list {
                if directive.kind != Dir::PartOf || directive.names_file() {
                    directive.target = index.resolve_uri(f, directive.uri);
                }
            }
        }
        index.directives = parsed;

        // Libraries: a `part of` file belongs to the file it names, else to the file that lists it.
        let mut owners: BTreeMap<usize, usize> = BTreeMap::new();
        for f in 0..files.len() {
            for directive in &index.directives[f] {
                if directive.kind == Dir::Part
                    && let Some(target) = directive.target
                    && target != f
                {
                    owners.entry(target).or_insert(f);
                }
            }
        }
        for g in 0..files.len() {
            let Some(part_of) = index.directives[g].iter().find(|d| d.kind == Dir::PartOf) else {
                continue;
            };
            let named = if part_of.names_file() { part_of.target } else { None };
            if let Some(owner) = named.or_else(|| owners.get(&g).copied())
                && owner != g
                && is_dart(world, owner)
            {
                index.lib_of[g] = owner;
            }
        }
        for g in 0..files.len() {
            if is_dart(world, g) {
                index.lib_files.entry(index.lib_of[g]).or_default().push(g);
            }
        }
        index
    }

    // ------------------------------------------------------------ files

    fn dart_file(&self, path: &str) -> Option<usize> {
        self.world.file_index(path).filter(|&f| is_dart(self.world, f))
    }

    fn scope_of(&self, file: usize) -> Option<usize> {
        self.scope_of_file.get(file).copied().flatten()
    }

    /// The file a URI names for the file that writes it.
    fn resolve_uri(&self, file: usize, uri: &str) -> Option<usize> {
        if let Some(rest) = uri.strip_prefix("package:") {
            let (package, path) = rest.split_once('/')?;
            return self.package_file(file, package, path);
        }
        if uri.contains(':') {
            return None;
        }
        let mut parts: Vec<&str> = self.world.files()[file].path.split('/').collect();
        parts.pop();
        for segment in uri.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                other => parts.push(other),
            }
        }
        let path = parts.join("/");
        if let Some(project) = self.scope_of(file) {
            let root = &self.world.projects()[project].root;
            if !root.is_empty()
                && !path
                    .strip_prefix(root.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
            {
                return None;
            }
        }
        self.dart_file(&path)
    }

    fn package_file(&self, file: usize, package: &str, path: &str) -> Option<usize> {
        let own = self.scope_of(file);
        let project = match own {
            Some(p) if self.world.projects()[p].name == package => p,
            _ => self.other_package(file, own, package)?,
        };
        let root = &self.world.projects()[project].root;
        let lib = if root.is_empty() {
            format!("lib/{path}")
        } else {
            format!("{root}/lib/{path}")
        };
        self.dart_file(&lib)
    }

    /// A project named `package` other than the file's own: a path dependency first, else the one
    /// nearest to the file. Several equally near ones name nothing.
    fn other_package(&self, file: usize, own: Option<usize>, package: &str) -> Option<usize> {
        let all = self.packages.get(package)?;
        let mut pool: Vec<usize> = all.clone();
        if let Some(own) = own {
            let referenced: Vec<usize> = all
                .iter()
                .copied()
                .filter(|p| self.visible.get(&own).is_some_and(|set| set.contains(p)))
                .collect();
            if !referenced.is_empty() {
                pool = referenced;
            }
        }
        if let [only] = pool.as_slice() {
            return Some(*only);
        }
        let from = &self.world.files()[file].path;
        let near = |p: &usize| common_dirs(from, &self.world.projects()[*p].path);
        let best = pool.iter().map(near).max()?;
        let mut chosen = pool.iter().copied().filter(|p| near(p) == best);
        match (chosen.next(), chosen.next()) {
            (Some(one), None) => Some(one),
            _ => None,
        }
    }

    /// Whether a fallback for code in project `from` may pick project `to`.
    fn sees(&self, from: usize, to: usize) -> bool {
        from == to || self.visible.get(&from).is_some_and(|set| set.contains(&to))
    }

    // ------------------------------------------------------------ names

    fn library_files(&self, file: usize) -> &[usize] {
        self.lib_files
            .get(&self.lib_of[file])
            .map_or(std::slice::from_ref(&self.lib_of[file]), Vec::as_slice)
    }

    /// Top-level declarations of the file's library, private ones too.
    fn lib_tops(&self, file: usize, name: &str) -> Vec<SymRef> {
        let mut out = Vec::new();
        for &lf in self.library_files(file) {
            if let Some(list) = self.tops[lf].get(name) {
                out.extend(list.iter().map(|&decl| SymRef { file: lf, decl }));
            }
        }
        out
    }

    /// The public names a library brings to an importer: its own and what it exports.
    fn exports_of(&self, root: usize) -> Rc<Names<'w>> {
        if let Some(cached) = self.exported.borrow().get(&root) {
            return Rc::clone(cached);
        }
        let names = Rc::new(self.exports_inner(root, &mut vec![root], 0));
        self.exported.borrow_mut().insert(root, Rc::clone(&names));
        names
    }

    fn exports_inner(&self, root: usize, path: &mut Vec<usize>, depth: u8) -> Names<'w> {
        let mut names: Names<'w> = BTreeMap::new();
        let single = [root];
        let library = self.lib_files.get(&root).map_or(&single[..], Vec::as_slice);
        for &lf in library {
            for (name, list) in &self.tops[lf] {
                if !name.starts_with('_') {
                    names
                        .entry(*name)
                        .or_default()
                        .extend(list.iter().map(|&decl| SymRef { file: lf, decl }));
                }
            }
        }
        if depth < 16 {
            for &lf in library {
                for directive in self.directives[lf].iter().filter(|d| d.kind == Dir::Export) {
                    let Some(target) = directive.target else { continue };
                    let target = self.lib_of[target];
                    if path.contains(&target) {
                        continue;
                    }
                    path.push(target);
                    let sub = self.exports_inner(target, path, depth + 1);
                    path.pop();
                    for (name, list) in sub {
                        if directive.allows(name) {
                            names.entry(name).or_default().extend(list);
                        }
                    }
                }
            }
        }
        for list in names.values_mut() {
            list.sort();
            list.dedup();
        }
        names
    }

    /// What the file's library imports under `prefix` (`None`: with no prefix) calls `name`.
    fn imported(&self, file: usize, prefix: Option<&str>, name: &str) -> Vec<SymRef> {
        let mut out = Vec::new();
        for &lf in self.library_files(file) {
            for directive in self.directives[lf]
                .iter()
                .filter(|d| d.kind == Dir::Import && d.prefix == prefix)
            {
                let Some(target) = directive.target else { continue };
                if !directive.allows(name) {
                    continue;
                }
                if let Some(list) = self.exports_of(self.lib_of[target]).get(name) {
                    out.extend(list.iter().copied());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn is_prefix(&self, file: usize, name: &str) -> bool {
        self.library_files(file)
            .iter()
            .any(|&lf| self.directives[lf].iter().any(|d| d.prefix == Some(name)))
    }

    /// A name written without a qualifier: the library's own, then its imports'.
    fn lookup_name(&self, file: usize, name: &str) -> Vec<SymRef> {
        let mut own = self.lib_tops(file, name);
        if own.is_empty() {
            return self.imported(file, None, name);
        }
        own.sort();
        own.dedup();
        own
    }

    fn resolve_type(&self, file: usize, written: TypeRef<'_>) -> Vec<SymRef> {
        let found = match written.prefix {
            Some(prefix) => self.imported(file, Some(prefix), written.name),
            None => self.lookup_name(file, written.name),
        };
        found
            .into_iter()
            .filter(|sym| self.world.decl(*sym).kind == DeclKind::Type)
            .collect()
    }

    // ------------------------------------------------------------ types

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

    fn member_of(&self, sym: SymRef, name: &str, depth: u8) -> Vec<SymRef> {
        if let Some(list) = self.members.get(&sym).and_then(|m| m.get(name)) {
            return list.clone();
        }
        if depth >= MAX_DEPTH {
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
            let mut segments: Vec<&str> = qualifier_segments(site);
            segments.push(site.name.as_str());
            if let Walked::Found(found) = self.walk(sym.file, None, &segments, true) {
                out.extend(
                    found
                        .into_iter()
                        .filter(|base| *base != sym && self.world.decl(*base).kind == DeclKind::Type),
                );
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The declared type of a field, a variable or a getter, from its signature.
    fn decl_type(&self, sym: SymRef) -> Option<TypeRef<'w>> {
        let decl = self.world.decl(sym);
        let tokens = split_words(&decl.signature);
        match decl.kind {
            DeclKind::Member => {
                let body = match tokens.split_last() {
                    Some((last, rest)) if *last == decl.name => rest,
                    _ => &tokens[..],
                };
                first_type(body)
            }
            DeclKind::Function | DeclKind::Method => {
                let at = tokens.iter().position(|token| *token == "get")?;
                first_type(&tokens[..at])
            }
            _ => None,
        }
    }

    /// The declared type of parameter `name` of the function around `scope`.
    fn param_type(&self, file: usize, scope: Option<usize>, name: &str) -> Option<TypeRef<'w>> {
        let decls = &self.world.files()[file].decls;
        let mut current = scope?;
        loop {
            let decl = decls.get(current)?;
            if matches!(decl.kind, DeclKind::Function | DeclKind::Method) {
                return param_type(&decl.signature, name);
            }
            current = decl.parent?;
        }
    }

    // ------------------------------------------------------------ walking

    /// Follows `segments` (a receiver path and the name called or referenced last) from the code
    /// at `scope` in `file`. `types_only` is for base lists: no members, no parameters.
    fn walk(&self, file: usize, scope: Option<usize>, segments: &[&str], types_only: bool) -> Walked {
        let Some(&first) = segments.first() else {
            return Walked::External;
        };
        let last = segments.len() - 1;
        let enclosing = self.enclosing_type(file, scope);
        let mut via_type = false;
        let mut cur: Vec<Cur> = Vec::new();

        if last > 0 && first == "this" {
            let Some(ty) = enclosing else {
                return Walked::Unknown;
            };
            cur.push(Cur::Type(ty));
        } else if last > 0 && first == "super" {
            let Some(ty) = enclosing else {
                return Walked::Unknown;
            };
            cur.extend(self.bases(ty).into_iter().map(Cur::Type));
            if cur.is_empty() {
                return Walked::External;
            }
        } else if last > 0 && self.is_prefix(file, first) {
            cur.push(Cur::Ns(first));
        } else {
            let param = if last > 0 && !types_only {
                self.param_type(file, scope, first)
            } else {
                None
            };
            if let Some(written) = param {
                let types = self.resolve_type(file, written);
                if types.is_empty() {
                    return Walked::External;
                }
                cur.extend(types.into_iter().map(Cur::Type));
            } else {
                let mut found = Vec::new();
                if !types_only && let Some(ty) = enclosing {
                    found = self.member_of(ty, first, 0);
                }
                let from_members = !found.is_empty();
                if found.is_empty() {
                    found = self.lookup_name(file, first);
                }
                if found.is_empty() {
                    return if last == 0 || first.starts_with(char::is_uppercase) {
                        Walked::External
                    } else {
                        Walked::Unknown
                    };
                }
                if last == 0 {
                    return Walked::Found(found);
                }
                for sym in found {
                    if self.world.decl(sym).kind == DeclKind::Type {
                        via_type = via_type || !from_members;
                        push_unique(&mut cur, Cur::Type(sym));
                    } else {
                        match self.value_types(sym) {
                            Some(types) => {
                                for ty in types {
                                    push_unique(&mut cur, Cur::Type(ty));
                                }
                            }
                            None => return Walked::Unknown,
                        }
                    }
                }
                if cur.is_empty() {
                    return Walked::External;
                }
            }
        }

        for (position, segment) in segments.iter().enumerate().skip(1) {
            let is_last = position == last;
            let mut hits: Vec<SymRef> = Vec::new();
            let mut missing: Vec<SymRef> = Vec::new();
            let mut next: Vec<Cur> = Vec::new();
            for step in &cur {
                let found = match step {
                    Cur::Type(ty) => {
                        let found = self.member_of(*ty, segment, 0);
                        if found.is_empty() {
                            missing.push(*ty);
                        }
                        found
                    }
                    Cur::Ns(prefix) => self.imported(file, Some(*prefix), segment),
                };
                if is_last {
                    hits.extend(found);
                    continue;
                }
                for sym in found {
                    if self.world.decl(sym).kind == DeclKind::Type {
                        push_unique(&mut next, Cur::Type(sym));
                    } else {
                        match self.value_types(sym) {
                            Some(types) => {
                                for ty in types {
                                    push_unique(&mut next, Cur::Type(ty));
                                }
                            }
                            None => return Walked::Unknown,
                        }
                    }
                }
            }
            if is_last {
                hits.sort();
                hits.dedup();
                return if !hits.is_empty() {
                    Walked::Found(hits)
                } else if missing.is_empty() {
                    Walked::External
                } else {
                    Walked::Missing {
                        types: missing,
                        via_type,
                    }
                };
            }
            if next.is_empty() {
                return if missing.is_empty() {
                    Walked::External
                } else {
                    Walked::Unknown
                };
            }
            cur = next;
        }
        Walked::External
    }

    /// The repo types a field, variable or getter holds: `None` when its type is not written,
    /// an empty list when it is a type outside the repo.
    fn value_types(&self, sym: SymRef) -> Option<Vec<SymRef>> {
        let written = self.decl_type(sym)?;
        Some(self.resolve_type(sym.file, written))
    }

    // ------------------------------------------------------------ sites

    fn resolve_file(&self, file: usize, edges: &mut Vec<Edge>) {
        let world = self.world;
        let facts = &world.files()[file];
        for directive in &self.directives[file] {
            let target = if directive.kind == Dir::PartOf && !directive.names_file() {
                Some(self.lib_of[file]).filter(|root| *root != file)
            } else {
                directive.target
            };
            if let Some(target) = target.filter(|target| *target != file) {
                edges.push(Edge {
                    src: Node::Decl(SymRef { file, decl: 0 }),
                    dst: Some(Node::Decl(SymRef { file: target, decl: 0 })),
                    dst_name: world.files()[target].decls[0].qname.clone(),
                    kind: RefKind::Imports,
                    exact: true,
                    line: directive.line,
                });
            }
        }
        for site in &facts.refs {
            let outcome = match site.kind {
                RefKind::Calls => self.resolve_call(file, site),
                RefKind::Inherits | RefKind::Implements => {
                    let mut segments = qualifier_segments(site);
                    segments.push(site.name.as_str());
                    match self.walk(file, site.scope, &segments, true) {
                        Walked::Found(found) => {
                            let types: Vec<SymRef> = found
                                .into_iter()
                                .filter(|sym| world.decl(*sym).kind == DeclKind::Type)
                                .collect();
                            if types.is_empty() {
                                Outcome::Skip
                            } else {
                                Outcome::Found(types, true)
                            }
                        }
                        _ => Outcome::Skip,
                    }
                }
                RefKind::References => {
                    let mut segments = qualifier_segments(site);
                    segments.push(site.name.as_str());
                    match self.walk(file, site.scope, &segments, false) {
                        Walked::Found(found) => Outcome::Found(found, true),
                        _ => Outcome::Skip,
                    }
                }
                // Imports are the directives above; only the dispatch pass makes the rest.
                RefKind::Imports | RefKind::Dispatch => Outcome::Skip,
            };
            let src = Node::Decl(SymRef {
                file,
                decl: site.scope.unwrap_or(0),
            });
            match outcome {
                Outcome::Skip => {}
                Outcome::Found(found, may_be_exact) => {
                    let distinct = found
                        .iter()
                        .map(|sym| world.decl(*sym).qname.as_str())
                        .collect::<BTreeSet<_>>()
                        .len();
                    for target in found {
                        edges.push(Edge {
                            src: src.clone(),
                            dst: Some(Node::Decl(target)),
                            dst_name: world.decl(target).qname.clone(),
                            kind: site.kind,
                            exact: may_be_exact && distinct == 1,
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

    fn resolve_call(&self, file: usize, site: &RefSite) -> Outcome {
        let name = site.name.as_str();
        let receiver;
        let mut segments: Vec<&str> = Vec::new();
        if let Some(qualifier) = site.qualifier.as_deref() {
            receiver = qualifier
                .strip_prefix('.')
                .unwrap_or(qualifier)
                .replace("?.", ".")
                .replace("!.", ".");
            let receiver = receiver.trim_end_matches(['!', '?']);
            match path_segments(receiver) {
                Some(path) => segments = path,
                None => return self.infer(file, name),
            }
        }
        segments.push(name);
        match self.walk(file, site.scope, &segments, false) {
            Walked::Found(found) => Outcome::Found(found, true),
            Walked::External => Outcome::Skip,
            Walked::Unknown => self.infer(file, name),
            Walked::Missing { types, via_type } => {
                if via_type {
                    Outcome::Found(types, false)
                } else {
                    self.infer(file, name)
                }
            }
        }
    }

    /// The name-only fallback for a member call on a receiver of unknown type.
    fn infer(&self, file: usize, name: &str) -> Outcome {
        if COMMON.contains(&name) {
            return Outcome::Skip;
        }
        if name.starts_with('_') {
            let root = self.lib_of[file];
            let same: Vec<SymRef> = self
                .methods_by_name
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|sym| self.lib_of[sym.file] == root)
                .collect();
            return match same.len() {
                1 => Outcome::Found(same, false),
                _ => Outcome::Skip,
            };
        }
        let scope = self.scope_of(file);
        let chosen = self.infer_candidates(scope, name);
        match chosen.len() {
            0 => Outcome::Skip,
            1 => Outcome::Found(chosen, false),
            _ => {
                let scope = scope.map_or_else(
                    || WORKSPACE_SCOPE.to_string(),
                    |project| self.world.projects()[project].path.clone(),
                );
                Outcome::Bucket(Bucket::new(scope, name))
            }
        }
    }

    /// Public methods called `name` in the project and the projects it references. Code outside
    /// every project only sees other code outside every project.
    fn infer_candidates(&self, scope: Option<usize>, name: &str) -> Vec<SymRef> {
        let Some(all) = self.methods_by_name.get(name) else {
            return Vec::new();
        };
        all.iter()
            .copied()
            .filter(|sym| match (scope, self.scope_of(sym.file)) {
                (Some(from), Some(to)) => self.sees(from, to),
                (None, None) => true,
                _ => false,
            })
            .collect()
    }
}

// ---------------------------------------------------------------- helpers

fn push_unique<'s>(list: &mut Vec<Cur<'s>>, item: Cur<'s>) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// The dotted qualifier of a site as segments (an import prefix, or a path for an annotation).
fn qualifier_segments(site: &RefSite) -> Vec<&str> {
    match site.qualifier.as_deref() {
        Some(qualifier) => qualifier.split('.').filter(|part| !part.is_empty()).collect(),
        None => Vec::new(),
    }
}

/// `a.b.c` as segments, or `None` when the receiver is an expression.
fn path_segments(receiver: &str) -> Option<Vec<&str>> {
    let segments: Vec<&str> = receiver.split('.').collect();
    let is_name = |s: &&str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    segments.iter().all(is_name).then_some(segments)
}

/// Whitespace-separated words that keep brackets together: `Map<String, int> x` is two.
fn split_words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut previous = ' ';
    let mut start: Option<usize> = None;
    for (at, c) in text.char_indices() {
        match c {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' if previous != '=' && previous != '-' => depth = depth.saturating_sub(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if let Some(from) = start.take() {
                out.push(&text[from..at]);
            }
        } else if start.is_none() {
            start = Some(at);
        }
        previous = c;
    }
    if let Some(from) = start {
        out.push(&text[from..]);
    }
    out
}

/// The type written first among `words`, once modifiers are skipped.
fn first_type<'a>(words: &[&'a str]) -> Option<TypeRef<'a>> {
    let word = words.iter().find(|word| !MODIFIERS.contains(*word))?;
    type_ref(word)
}

/// `Foo`, `Foo?`, `Foo<Bar>`, `p.Foo` as a type; `None` for a record, a function type or `var`.
fn type_ref(word: &str) -> Option<TypeRef<'_>> {
    let word = word.trim_end_matches(['?', '*']);
    let word = word.split('<').next()?;
    if word.is_empty()
        || word == "var"
        || !word
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '$'))
    {
        return None;
    }
    Some(match word.rsplit_once('.') {
        Some((prefix, name)) if !prefix.is_empty() && !name.is_empty() => TypeRef {
            prefix: Some(prefix),
            name,
        },
        _ => TypeRef {
            prefix: None,
            name: word,
        },
    })
}

/// The declared type of parameter `name` in a function signature `ret f(T a, {U b, required V c})`.
fn param_type<'a>(signature: &'a str, name: &str) -> Option<TypeRef<'a>> {
    let open = signature.find('(')?;
    let list = &signature[open + 1..];
    let mut depth = 0usize;
    let mut previous = ' ';
    let mut start = 0usize;
    let mut parts: Vec<&str> = Vec::new();
    for (at, c) in list.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' if previous != '=' && previous != '-' => depth = depth.saturating_sub(1),
            ')' if depth == 0 => {
                parts.push(&list[start..at]);
                start = list.len();
                break;
            }
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        previous = c;
    }
    if start < list.len() {
        parts.push(&list[start..]);
    }
    for part in parts {
        let part = part.trim_matches(|c: char| c.is_whitespace() || matches!(c, '{' | '}' | '[' | ']'));
        let part = part.split('=').next().unwrap_or(part);
        let words = split_words(part);
        let Some((last, rest)) = words.split_last() else {
            continue;
        };
        if *last == name {
            return first_type(rest);
        }
    }
    None
}

/// How many leading directories two paths share.
fn common_dirs(a: &str, b: &str) -> usize {
    let da: Vec<&str> = a.split('/').collect();
    let db: Vec<&str> = b.split('/').collect();
    let (da, db) = (&da[..da.len() - 1], &db[..db.len() - 1]);
    da.iter().zip(db.iter()).take_while(|(x, y)| x == y).count()
}
