//! Resolution: turning reference sites into edges. The [`World`] holds every
//! file's facts and every project's facts; a [`Resolver`] per language reads
//! it and returns [`Edge`]s. Nothing here touches SQLite or the filesystem.

use std::collections::{BTreeMap, BTreeSet};

use crate::dispatch::dispatch;
use crate::facts::{Decl, FileFacts, Lang, RefKind};
use crate::project::ProjectFacts;
use crate::resolve_csharp::CSharpResolver;
use crate::resolve_dart::DartResolver;
use crate::resolve_python::PythonResolver;
use crate::resolve_rust::RustResolver;
use crate::resolve_ts::TsResolver;

/// A declaration: its file's index in the [`World`] and its index in that file's `decls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymRef {
    pub file: usize,
    pub decl: usize,
}

/// A name bucket: an untyped site with several candidates points at one node
/// instead of an edge per candidate, and the bucket lists those candidates
/// once. `scope` is the visibility the fallback was computed in: a crate or
/// project path, or [`WORKSPACE_SCOPE`] when it sees everything.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bucket {
    pub scope: String,
    pub name: String,
}

/// The scope of a fallback with no crate or project around it.
pub const WORKSPACE_SCOPE: &str = "*";

impl Bucket {
    pub fn new(scope: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            name: name.into(),
        }
    }

    /// `name:<scope>:<name>`: how the node reads in an edge's `dst_name`.
    pub fn qname(&self) -> String {
        format!("name:{}:{}", self.scope, self.name)
    }
}

/// One end of an edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Node {
    Decl(SymRef),
    /// Index into [`World::projects`].
    Project(usize),
    Bucket(Bucket),
}

/// A resolved (or unresolved) reference. `dst` is `None` when nothing was found;
/// `dst_name` is then the name as written, else the target's qualified name.
/// `exact` means resolved to one definition without the name-only fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub src: Node,
    pub dst: Option<Node>,
    pub dst_name: String,
    pub kind: RefKind,
    pub exact: bool,
    pub line: u32,
}

impl Edge {
    fn sort_key(&self) -> (&Node, u32, &'static str, &str, Option<&Node>, bool) {
        (
            &self.src,
            self.line,
            self.kind.as_str(),
            &self.dst_name,
            self.dst.as_ref(),
            self.exact,
        )
    }
}

/// Sort edges into their one deterministic order and drop duplicates.
pub fn finish(mut edges: Vec<Edge>) -> Vec<Edge> {
    edges.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    edges.dedup();
    edges
}

/// All facts of one repository state. Files and projects are kept sorted by
/// path, so indices and results do not depend on the order they came in.
#[derive(Debug, Clone, Default)]
pub struct World {
    files: Vec<FileFacts>,
    projects: Vec<ProjectFacts>,
}

impl World {
    pub fn new(mut files: Vec<FileFacts>, mut projects: Vec<ProjectFacts>) -> Self {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        projects.sort_by(|a, b| a.path.cmp(&b.path));
        Self { files, projects }
    }

    pub fn files(&self) -> &[FileFacts] {
        &self.files
    }

    pub fn projects(&self) -> &[ProjectFacts] {
        &self.projects
    }

    pub fn decl(&self, sym: SymRef) -> &Decl {
        &self.files[sym.file].decls[sym.decl]
    }

    pub fn file_index(&self, path: &str) -> Option<usize> {
        self.files.binary_search_by(|file| file.path.as_str().cmp(path)).ok()
    }
}

/// One language's resolution rules.
pub trait Resolver: Sync {
    /// Edges for the sites of the files at `files` (indices into the world),
    /// in [`finish`] order. Everything else in the world is read for context.
    fn resolve_files(&self, world: &World, files: &[usize]) -> Vec<Edge>;

    /// The declarations each bucket of `buckets` holds, in the same order: the
    /// name fallback's candidates as seen from that scope.
    fn bucket_members(&self, _world: &World, buckets: &[Bucket]) -> Vec<Vec<SymRef>> {
        let _ = buckets;
        Vec::new()
    }

    /// Edges between project files (`ProjectReference` and the like).
    fn resolve_projects(&self, _world: &World) -> Vec<Edge> {
        Vec::new()
    }
}

static CSHARP: CSharpResolver = CSharpResolver;
static RUST: RustResolver = RustResolver;
static TS: TsResolver = TsResolver;
static PYTHON: PythonResolver = PythonResolver;
static DART: DartResolver = DartResolver;

/// The resolver for a language. TypeScript and JavaScript share one.
pub fn resolver_for(lang: Lang) -> &'static dyn Resolver {
    match lang {
        Lang::CSharp => &CSHARP,
        Lang::Rust => &RUST,
        Lang::TypeScript | Lang::JavaScript => &TS,
        Lang::Python => &PYTHON,
        Lang::Dart => &DART,
    }
}

const LANGS: [Lang; 6] = [
    Lang::CSharp,
    Lang::Rust,
    Lang::TypeScript,
    Lang::JavaScript,
    Lang::Python,
    Lang::Dart,
];

/// Every edge of the world: all files and all projects. Buckets are expanded
/// to their members here, so a caller reads one edge per candidate.
pub fn resolve(world: &World) -> Vec<Edge> {
    let mut edges = Vec::new();
    for lang in LANGS {
        let files: Vec<usize> = (0..world.files().len())
            .filter(|&index| world.files()[index].lang == lang)
            .collect();
        edges.extend(resolver_for(lang).resolve_files(world, &files));
    }
    for lang in [Lang::CSharp, Lang::Rust, Lang::TypeScript, Lang::Python] {
        edges.extend(resolver_for(lang).resolve_projects(world));
    }
    let mut edges = finish(expand_buckets(world, edges));
    let extra = dispatch(world, &edges);
    edges.extend(extra.into_iter().map(|(_, edge)| edge));
    finish(edges)
}

/// Replaces every edge into a bucket with one inferred edge per member. The
/// stored graph keeps the bucket instead; this is for callers that want the
/// flat answer.
fn expand_buckets(world: &World, edges: Vec<Edge>) -> Vec<Edge> {
    let mut needed: BTreeMap<Lang, BTreeSet<Bucket>> = BTreeMap::new();
    for edge in &edges {
        if let (Node::Decl(src), Some(Node::Bucket(bucket))) = (&edge.src, &edge.dst) {
            needed
                .entry(world.files()[src.file].lang)
                .or_default()
                .insert(bucket.clone());
        }
    }
    let mut members: BTreeMap<(Lang, Bucket), Vec<SymRef>> = BTreeMap::new();
    for (lang, buckets) in needed {
        let list: Vec<Bucket> = buckets.into_iter().collect();
        for (bucket, found) in list
            .iter()
            .cloned()
            .zip(resolver_for(lang).bucket_members(world, &list))
        {
            members.insert((lang, bucket), found);
        }
    }
    let mut out = Vec::with_capacity(edges.len());
    for edge in edges {
        let Some(Node::Bucket(bucket)) = &edge.dst else {
            out.push(edge);
            continue;
        };
        let Node::Decl(src) = edge.src else {
            continue;
        };
        let lang = world.files()[src.file].lang;
        let found = members.get(&(lang, bucket.clone())).cloned().unwrap_or_default();
        if found.is_empty() {
            out.push(Edge {
                src: Node::Decl(src),
                dst: None,
                kind: edge.kind,
                exact: false,
                line: edge.line,
                dst_name: edge.dst_name,
            });
            continue;
        }
        for member in found {
            out.push(Edge {
                src: Node::Decl(src),
                dst: Some(Node::Decl(member)),
                dst_name: world.decl(member).qname.clone(),
                kind: edge.kind,
                exact: false,
                line: edge.line,
            });
        }
    }
    out
}
