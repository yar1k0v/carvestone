//! The code graph in memory, for the questions that walk it: what the domain model is, and what the
//! symbol pages show.
//!
//! The `edge_ends` view answers one symbol quickly and the whole graph slowly, so the map reads the
//! raw `edges` and `native_edges` rows once and applies the same rules: a native edge stands in for
//! the tree-sitter edge it resolves, and a guess between two files a native run covered is left
//! out. A name bucket counts only while it is small: a guess among many says nothing.
//!
//! One map is kept per repository until the graph moves.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::Connection;

use crate::code_index::meta_value;
use crate::code_rank::is_test_path;
use crate::owner::{Element, arch_elements};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// A bucket with more candidates than this is no evidence of a link.
const MAX_BUCKET: usize = 3;
/// Edge kinds a guess between two natively covered files survives for (see `edge_ends`).
const KEPT_GUESSES: [&str; 1] = ["dispatch"];

/// One declaration.
#[derive(Debug, Clone)]
pub(crate) struct Sym {
    pub id: i64,
    pub qname: String,
    pub name: String,
    pub kind: String,
    pub parent: Option<i64>,
    pub file: i64,
    pub line_start: u32,
    pub line_end: u32,
    pub lang: String,
    pub signature: String,
    pub doc: Option<String>,
}

impl Sym {
    /// The words of the header, to ask what kind of type it is.
    pub(crate) fn has_word(&self, word: &str) -> bool {
        self.signature
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == word)
    }
}

/// Test code: what the graph calls test code, and a project named for tests
/// (`Shop.Tests.Integration/`).
pub(crate) fn is_test(path: &str) -> bool {
    is_test_path(path)
        || path.split('/').rev().skip(1).any(|dir| {
            let dir = dir.to_ascii_lowercase();
            dir.contains(".tests.") || dir.contains(".test.")
        })
}

/// The markup words of a C# doc comment. Any other `<..>` is text, such as `List<T>`.
const DOC_TAGS: [&str; 18] = [
    "summary",
    "remarks",
    "returns",
    "param",
    "paramref",
    "typeparam",
    "typeparamref",
    "see",
    "seealso",
    "c",
    "code",
    "para",
    "value",
    "example",
    "exception",
    "inheritdoc",
    "list",
    "item",
];

/// A doc comment as plain text: no markup, a `<see cref="A.B"/>` reads as `B`.
pub(crate) fn plain_doc(doc: &str) -> String {
    let mut out = String::new();
    let mut rest = doc;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('>') else {
            rest = &rest[at..];
            break;
        };
        let tag = &rest[at + 1..at + end];
        let word = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or("");
        if DOC_TAGS.contains(&word) {
            if let Some((_, value)) = tag.split_once('"') {
                let value = value.split('"').next().unwrap_or(value);
                if !tag.starts_with("param ") && !tag.starts_with("typeparam ") {
                    out.push_str(value.rsplit(['.', ':']).next().unwrap_or(value));
                }
            }
        } else {
            out.push_str(&rest[at..=at + end]);
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    let lines: Vec<&str> = out.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    lines.join("\n")
}

/// One end of an edge, as seen from the other end.
#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub other: i64,
    pub kind: String,
    pub exact: bool,
    pub line: u32,
}

/// The graph of one repository at one moment.
#[derive(Default)]
pub(crate) struct CodeMap {
    pub syms: HashMap<i64, Sym>,
    pub paths: HashMap<i64, String>,
    pub langs: HashMap<i64, String>,
    pub out: HashMap<i64, Vec<Link>>,
    pub incoming: HashMap<i64, Vec<Link>>,
    pub children: HashMap<i64, Vec<i64>>,
    pub by_name: HashMap<String, Vec<i64>>,
    pub by_qname: HashMap<String, Vec<i64>>,
    /// Every package a project file names, lower-cased.
    pub packages: HashSet<String>,
}

impl CodeMap {
    pub(crate) fn path(&self, sym: &Sym) -> &str {
        self.paths.get(&sym.file).map_or("", String::as_str)
    }

    pub(crate) fn sym_is_test(&self, sym: &Sym) -> bool {
        is_test(self.path(sym))
    }

    /// The type around `id`, or `id` itself when it is one; `None` outside every type.
    pub(crate) fn owner_type(&self, id: i64) -> Option<&Sym> {
        let mut at = Some(id);
        while let Some(current) = at {
            let sym = self.syms.get(&current)?;
            if sym.kind == "type" {
                return Some(sym);
            }
            at = sym.parent;
        }
        None
    }

    pub(crate) fn out_of(&self, id: i64) -> &[Link] {
        self.out.get(&id).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn links_to(&self, id: i64) -> &[Link] {
        self.incoming.get(&id).map_or(&[], Vec::as_slice)
    }

    /// The declarations directly inside `id`.
    pub(crate) fn inside(&self, id: i64) -> impl Iterator<Item = &Sym> {
        self.children
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|child| self.syms.get(child))
    }
}

fn load(conn: &Connection) -> Result<CodeMap, EngineError> {
    let mut map = CodeMap::default();
    let mut stmt = conn.prepare("SELECT id, path, lang FROM files WHERE lang IS NOT NULL")?;
    let files = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for file in files {
        let (id, path, lang) = file?;
        map.paths.insert(id, path);
        map.langs.insert(id, lang);
    }

    let mut stmt = conn.prepare(
        "SELECT id, qname, name, kind, parent_id, file_id, line_start, line_end, lang, signature, doc \
         FROM symbols",
    )?;
    let syms = stmt.query_map([], |row| {
        Ok(Sym {
            id: row.get(0)?,
            qname: row.get(1)?,
            name: row.get(2)?,
            kind: row.get(3)?,
            parent: row.get(4)?,
            file: row.get(5)?,
            line_start: row.get(6)?,
            line_end: row.get(7)?,
            lang: row.get(8)?,
            signature: row.get(9)?,
            doc: row.get(10)?,
        })
    })?;
    for sym in syms {
        let sym = sym?;
        if let Some(parent) = sym.parent {
            map.children.entry(parent).or_default().push(sym.id);
        }
        map.by_name.entry(sym.name.clone()).or_default().push(sym.id);
        map.by_qname.entry(sym.qname.clone()).or_default().push(sym.id);
        map.syms.insert(sym.id, sym);
    }
    for list in map.children.values_mut() {
        list.sort_unstable();
    }

    load_edges(conn, &mut map)?;

    let mut stmt = conn.prepare("SELECT name FROM project_items WHERE kind = 'package'")?;
    let packages = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for package in packages {
        map.packages.insert(package?.to_ascii_lowercase());
    }
    Ok(map)
}

/// A site's native edges: the targets, and the identifiers written there.
#[derive(Default)]
struct NativeSite {
    targets: HashSet<i64>,
    names: Vec<(String, bool)>,
}

/// True when `dst_name` is `name` or ends with it after a separator.
fn names_same(dst_name: &str, name: &str) -> bool {
    dst_name == name
        || dst_name
            .strip_suffix(name)
            .is_some_and(|head| head.ends_with(['.', ':', '/', '#']))
}

fn load_edges(conn: &Connection, map: &mut CodeMap) -> Result<(), EngineError> {
    let mut sites: HashMap<(i64, String, i64, u32), NativeSite> = HashMap::new();
    let mut dispatched: HashSet<(i64, i64)> = HashSet::new();
    let mut stmt = conn.prepare("SELECT src_id, dst_id, kind, name, file_id, line FROM native_edges")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, u32>(5)?,
        ))
    })?;
    let mut edges: Vec<(i64, i64, Link)> = Vec::new();
    for row in rows {
        let (src, dst, kind, name, file, line) = row?;
        if kind == "dispatch"
            && let Some(dst) = dst
        {
            dispatched.insert((src, dst));
        }
        let site = sites.entry((src, kind.clone(), file, line)).or_default();
        site.targets.extend(dst);
        site.names.push((name, dst.is_some()));
        if let Some(dst) = dst {
            edges.push((
                src,
                dst,
                Link {
                    other: 0,
                    kind,
                    exact: true,
                    line,
                },
            ));
        }
    }

    let mut covered: HashSet<i64> = HashSet::new();
    let mut stmt = conn.prepare("SELECT file_id FROM native_files WHERE covered = 1")?;
    for file in stmt.query_map([], |row| row.get::<_, i64>(0))? {
        covered.insert(file?);
    }
    let mut buckets: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT bucket_id, symbol_id FROM bucket_members")?;
    let members = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
    for member in members {
        let (bucket, symbol) = member?;
        buckets.entry(bucket).or_default().push(symbol);
    }

    let mut stmt =
        conn.prepare("SELECT src_id, dst_id, dst_bucket, dst_name, kind, exact, file_id, line FROM edges")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, u32>(7)?,
        ))
    })?;
    for row in rows {
        let (src, dst, bucket, dst_name, kind, exact, file, line) = row?;
        if kind == "dispatch" {
            if dst.is_some_and(|dst| dispatched.contains(&(src, dst))) {
                continue;
            }
        } else if let Some(site) = sites.get(&(src, kind.clone(), file, line)) {
            let same_target = dst.is_some_and(|dst| site.targets.contains(&dst));
            let same_name = site
                .names
                .iter()
                .any(|(name, resolved)| (*resolved || !exact) && names_same(&dst_name, name));
            if same_target || same_name {
                continue;
            }
        }
        let targets: Vec<i64> = match (dst, bucket) {
            (Some(dst), _) => vec![dst],
            (None, Some(bucket)) => match buckets.get(&bucket) {
                Some(members) if members.len() <= MAX_BUCKET => members.clone(),
                _ => continue,
            },
            _ => continue,
        };
        let kept = KEPT_GUESSES.contains(&kind.as_str());
        for target in targets {
            let both_covered =
                covered.contains(&file) && map.syms.get(&target).is_some_and(|sym| covered.contains(&sym.file));
            if !exact && !kept && both_covered {
                continue;
            }
            edges.push((
                src,
                target,
                Link {
                    other: 0,
                    kind: kind.clone(),
                    exact: exact && bucket.is_none(),
                    line,
                },
            ));
        }
    }
    for (src, dst, link) in edges {
        map.out.entry(src).or_default().push(Link {
            other: dst,
            ..link.clone()
        });
        map.incoming.entry(dst).or_default().push(Link { other: src, ..link });
    }
    for list in map.out.values_mut().chain(map.incoming.values_mut()) {
        list.sort_by(|a, b| (a.line, a.other, &a.kind).cmp(&(b.line, b.other, &b.kind)));
    }
    Ok(())
}

/// The kept map of one repository: where it is, the state of the graph, the map.
type Kept = (PathBuf, (String, i64), Arc<CodeMap>);

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// The map of `repo` as `conn` sees it now. `conn` must be up to date.
pub(crate) fn code_map(repo: &Repo, conn: &Connection) -> Result<Arc<CodeMap>, EngineError> {
    let root = repo.store.layout.root.clone();
    let version = meta_value(conn, "graph_version")?.unwrap_or_default();
    let native: i64 = conn.query_row("SELECT count(*) FROM native_edges", [], |row| row.get(0))?;
    let stamp = (version, native);
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((path, seen, map)) = kept.as_ref()
        && *path == root
        && *seen == stamp
    {
        return Ok(Arc::clone(map));
    }
    let map = Arc::new(load(conn)?);
    *kept = Some((root, stamp, Arc::clone(&map)));
    Ok(map)
}

/// Which files a request means: those of one architecture part and the parts inside it, or all.
pub(crate) struct Scope {
    elements: Vec<Element>,
    /// The ids of the parts the request names; `None` for the whole repository.
    wanted: Option<Vec<String>>,
}

/// The value of `part` that means no part at all.
const NO_PART: &str = "none";

impl Scope {
    /// `part` is an architecture id, or absent or `none` for the whole repository.
    pub(crate) fn new(snapshot: &Snapshot, part: Option<&str>) -> Result<Self, EngineError> {
        let elements = arch_elements(snapshot, &mut Vec::new());
        let part = part.map(str::trim).filter(|p| !p.is_empty() && *p != NO_PART);
        let Some(part) = part else {
            return Ok(Self { elements, wanted: None });
        };
        let (entry, _) = snapshot.resolve(part)?;
        let mut wanted = vec![entry.id.as_str().to_string()];
        // The parts below it, however deep: a container may leave its code to its components.
        let mut grew = true;
        while grew {
            grew = false;
            for other in &snapshot.entries {
                let parent = other
                    .item
                    .as_ref()
                    .and_then(|item| item.fields.parent.as_deref())
                    .and_then(|parent| snapshot.resolve(parent).ok())
                    .map(|(parent, _)| parent.id.as_str().to_string());
                let id = other.id.as_str().to_string();
                if parent.is_some_and(|parent| wanted.contains(&parent)) && !wanted.contains(&id) {
                    wanted.push(id);
                    grew = true;
                }
            }
        }
        Ok(Self {
            elements,
            wanted: Some(wanted),
        })
    }

    /// Whether the request covers `path`.
    pub(crate) fn covers(&self, path: &str) -> bool {
        let Some(wanted) = &self.wanted else {
            return true;
        };
        self.elements
            .iter()
            .any(|element| wanted.contains(&element.id) && element.globs.is_match(path))
    }
}
