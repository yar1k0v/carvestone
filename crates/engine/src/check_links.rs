//! Link and resolution rules of `stone check`.

use std::collections::HashMap;
use std::path::Path;

use carvestone_core::{body_links, diagrams, md_links};
use carvestone_graph::lang_of;
use carvestone_store::{Config, Entry};
use rusqlite::{Connection, OptionalExtension};

use crate::check::Problem;
use crate::code::open;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// Candidates an ambiguity message lists.
const CANDIDATES_SHOWN: usize = 10;

/// What a `[[code:...]]` target points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLink {
    /// One symbol, by qualified name.
    Symbol(String),
    /// A whole file.
    File(String),
    /// Lines `a` to `b` of a file.
    Range {
        path: String,
        a: u32,
        b: u32,
    },
    NotFound(String),
    /// Several symbols share the written name.
    Ambiguous {
        name: String,
        candidates: Vec<String>,
    },
    /// The range runs past the end of a file with `lines` lines.
    BadRange {
        path: String,
        a: u32,
        b: u32,
        lines: u32,
    },
    /// The range is empty or starts at line 0.
    BadSpan {
        a: u32,
        b: u32,
    },
    /// Nothing can be said: the target is not found, but the graph holds no symbols for its language.
    Unchecked,
}

impl CodeLink {
    /// The `check` message, `None` for a link that resolves or cannot be judged.
    pub fn problem(&self) -> Option<String> {
        match self {
            CodeLink::Symbol(_) | CodeLink::File(_) | CodeLink::Range { .. } | CodeLink::Unchecked => None,
            CodeLink::NotFound(name) => Some(format!("{name} not found")),
            CodeLink::Ambiguous { name, candidates } => {
                let more = if candidates.len() > CANDIDATES_SHOWN {
                    ", ..."
                } else {
                    ""
                };
                let shown: Vec<&str> = candidates.iter().take(CANDIDATES_SHOWN).map(String::as_str).collect();
                Some(format!(
                    "{name} matches {} symbols; use one of: {}{more}",
                    candidates.len(),
                    shown.join(", ")
                ))
            }
            CodeLink::BadRange { path, a, b, lines } => {
                Some(format!("range {a}-{b} is past the end of {path} ({lines} lines)"))
            }
            CodeLink::BadSpan { a, b } => Some(format!(
                "range {a}-{b} is not valid; write start-end with 1 <= start <= end"
            )),
        }
    }
}

/// Where a link resolves, with the line counts of files read on the way.
pub(crate) struct CodeResolver<'a> {
    root: &'a Path,
    conn: &'a Connection,
    line_counts: HashMap<String, Option<u32>>,
}

impl<'a> CodeResolver<'a> {
    pub(crate) fn new(root: &'a Path, conn: &'a Connection) -> CodeResolver<'a> {
        CodeResolver {
            root,
            conn,
            line_counts: HashMap::new(),
        }
    }

    /// Lines of a file, `None` when it is not on disk.
    pub(crate) fn lines_of(&mut self, path: &str) -> Option<u32> {
        let root = self.root;
        *self.line_counts.entry(path.to_string()).or_insert_with(|| {
            let bytes = std::fs::read(root.join(path)).ok()?;
            let newlines = bytes.iter().filter(|b| **b == b'\n').count();
            let tail = usize::from(bytes.last().is_some_and(|b| *b != b'\n'));
            Some((newlines + tail) as u32)
        })
    }

    fn has_symbols(&self, lang: Option<&str>) -> Result<bool, EngineError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM symbols WHERE kind NOT IN ('file', 'project') AND (?1 IS NULL OR lang = ?1) LIMIT 1",
                [lang],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    }

    /// `Qualified.Name` (exact qname, else a unique plain name), `path/file.ext` or `path/file.ext:a-b`.
    pub(crate) fn resolve(&mut self, target: &str) -> Result<CodeLink, EngineError> {
        let text = target.trim().replace('\\', "/");
        let (path, span) = split_span(&text);
        if span.is_some() || self.is_file(path)? {
            return self.resolve_file(path, span);
        }
        let by_qname: Option<String> = self
            .conn
            .query_row(
                "SELECT qname FROM symbols WHERE qname = ?1 AND kind != 'file' LIMIT 1",
                [&text],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(qname) = by_qname {
            return Ok(CodeLink::Symbol(qname));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT qname FROM symbols WHERE name = ?1 AND kind != 'file' ORDER BY qname")?;
        let mut names: Vec<String> = stmt.query_map([&text], |row| row.get(0))?.collect::<Result<_, _>>()?;
        match names.len() {
            1 => Ok(CodeLink::Symbol(names.remove(0))),
            0 => self.not_found(&text),
            _ => Ok(CodeLink::Ambiguous {
                name: text,
                candidates: names,
            }),
        }
    }

    fn is_file(&mut self, path: &str) -> Result<bool, EngineError> {
        let indexed = self
            .conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 AND lang IS NOT NULL",
                [path],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        Ok(indexed || self.lines_of(path).is_some())
    }

    fn resolve_file(&mut self, path: &str, span: Option<(u32, u32)>) -> Result<CodeLink, EngineError> {
        let Some(lines) = self.lines_of(path) else {
            return self.not_found(path);
        };
        let Some((a, b)) = span else {
            return Ok(CodeLink::File(path.to_string()));
        };
        if a == 0 || b < a {
            return Ok(CodeLink::BadSpan { a, b });
        }
        if b > lines {
            return Ok(CodeLink::BadRange {
                path: path.to_string(),
                a,
                b,
                lines,
            });
        }
        Ok(CodeLink::Range {
            path: path.to_string(),
            a,
            b,
        })
    }

    /// A miss only counts when the graph has symbols for the target's language; a name has no
    /// extension to tell, so any symbols do.
    fn not_found(&self, name: &str) -> Result<CodeLink, EngineError> {
        let known = self.has_symbols(lang_of(name).map(|lang| lang.as_str()))?;
        Ok(if known {
            CodeLink::NotFound(name.to_string())
        } else {
            CodeLink::Unchecked
        })
    }
}

/// `path:a-b` or `path:a` split into the path and its lines; the text unchanged when it has no span.
fn split_span(text: &str) -> (&str, Option<(u32, u32)>) {
    let Some((path, tail)) = text.rsplit_once(':') else {
        return (text, None);
    };
    let (first, last) = tail.split_once('-').unwrap_or((tail, tail));
    match (first.parse::<u32>(), last.parse::<u32>()) {
        (Ok(a), Ok(b)) if !path.is_empty() => (path, Some((a, b))),
        _ => (text, None),
    }
}

/// Where one `[[code:...]]` target resolves.
pub fn resolve_code_link(repo: &Repo, target: &str) -> Result<CodeLink, EngineError> {
    let index = open(repo)?;
    CodeResolver::new(&repo.store.layout.root, index.conn()).resolve(target)
}

/// `code-link` problems of every `[[code:...]]` in the store and in the doc-root files.
pub(crate) fn code_link_problems(
    root: &Path,
    snapshot: &Snapshot,
    docs: &[Entry],
    conn: &Connection,
    out: &mut Vec<Problem>,
) -> Result<(), EngineError> {
    let mut resolver = CodeResolver::new(root, conn);
    for entry in snapshot.entries.iter().chain(docs) {
        // An item exists only when the front matter parsed; a doc-root file always has one.
        let Some(item) = &entry.item else {
            continue;
        };
        for (line, target) in body_links(&item.body, item.body_line) {
            let Some(code) = target.strip_prefix("code:") else {
                continue;
            };
            if let Some(message) = resolver.resolve(code)?.problem() {
                out.push(Problem::new(&entry.path, line, "code-link", message));
            }
        }
    }
    Ok(())
}

/// Frontmatter keys whose values name other items.
pub(crate) const RELATION_KEYS: [&str; 8] = [
    "links",
    "depends_on",
    "components",
    "uses",
    "milestone",
    "plan",
    "supersedes",
    "parent",
];

/// The values of one `RELATION_KEYS` field.
pub(crate) fn relation_values<'a>(f: &'a carvestone_core::Fields, key: &str) -> Vec<&'a String> {
    match key {
        "links" => f.links.iter().collect(),
        "depends_on" => f.depends_on.iter().collect(),
        "components" => f.components.iter().collect(),
        "uses" => f.uses.iter().collect(),
        "milestone" => f.milestone.iter().collect(),
        "plan" => f.plan.iter().collect(),
        "supersedes" => f.supersedes.iter().collect(),
        _ => f.parent.iter().collect(),
    }
}

/// `reserved` holds the store paths of the `index.md` and `log.md` files, which are no items but valid
/// markdown link targets.
pub(crate) fn link_problems(
    snapshot: &Snapshot,
    docs: &[Entry],
    config: &Config,
    reserved: &[String],
    out: &mut Vec<Problem>,
) {
    let ctx = LinkCtx {
        snapshot,
        docs,
        config,
        reserved,
    };
    for entry in &snapshot.entries {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        let f = &item.fields;
        for key in RELATION_KEYS {
            let line = doc.key_line(key).unwrap_or(1);
            for value in relation_values(f, key).into_iter().filter(|v| !v.trim().is_empty()) {
                check_target(&ctx, entry, line, "relation", value.trim(), out);
            }
        }
        body_link_problems(&ctx, entry, item, out);
    }
    // Doc-root files have no fields to check, but their links must work.
    for entry in docs {
        if let Some(item) = &entry.item {
            body_link_problems(&ctx, entry, item, out);
        }
    }
}

/// What link checking resolves against: the store items and the read-only doc-root files.
struct LinkCtx<'a> {
    snapshot: &'a Snapshot,
    docs: &'a [Entry],
    config: &'a Config,
    reserved: &'a [String],
}

fn body_link_problems(ctx: &LinkCtx, entry: &Entry, item: &carvestone_core::Item, out: &mut Vec<Problem>) {
    for (line, target) in body_links(&item.body, item.body_line) {
        if !target.starts_with("code:") {
            check_target(ctx, entry, line, "body-link", &target, out);
        }
    }
    for (line, target) in md_links(&item.body, item.body_line) {
        check_target(ctx, entry, line, "md-link", &target, out);
    }
    for diagram in diagrams(&item.body, item.body_line) {
        for (line, target) in &diagram.links {
            check_target(ctx, entry, *line, "diagram-link", target, out);
        }
    }
}

/// One link: it must resolve, and so must its `#anchor` when the target's
/// sections are known.
fn check_target(ctx: &LinkCtx, from: &Entry, line: usize, rule: &'static str, target: &str, out: &mut Vec<Problem>) {
    let snapshot = ctx.snapshot;
    let (found, anchor) = if rule == "md-link" {
        let (path, anchor) = match target.split_once('#') {
            Some((p, a)) => (p, Some(a.to_string())),
            None => (target, None),
        };
        let Some(id) = link_id(Some(ctx.config), &from.path, path) else {
            return;
        };
        let known = snapshot.entries.iter().chain(ctx.docs).find(|e| e.id.as_str() == id);
        match known {
            Some(found) => (found, anchor),
            None if ctx.reserved.iter().any(|r| *r == format!("stone/{id}.md")) => return,
            None => {
                let place = if id.ends_with(".md") { "" } else { " in stone/" };
                out.push(Problem::new(
                    &from.path,
                    line,
                    rule,
                    format!("`{target}` does not exist{place}"),
                ));
                return;
            }
        }
    } else {
        match snapshot.resolve(target) {
            Ok(r) => r,
            Err(e) => {
                out.push(Problem::new(&from.path, line, rule, e.to_string()));
                return;
            }
        }
    };
    if let (Some(anchor), Some(_)) = (anchor, &found.item)
        && let Err(e) = snapshot.find_section(found, &anchor)
    {
        out.push(Problem::new(&from.path, line, "anchor", e.to_string()));
    }
}

/// The id a relative `.md` link points at, from the root-relative path of the file that holds
/// it: a store item (`stone/docs/x.md` is `docs/x`) or a plain doc, a doc-root or project file
/// (its path, `.md` kept). `None` when the link climbs out of the root or lands anywhere else.
pub(crate) fn link_id(config: Option<&Config>, from_path: &str, target: &str) -> Option<String> {
    let folder = from_path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut parts: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let path = parts.join("/");
    if let Some(inside) = path.strip_prefix("stone/") {
        return Some(inside.strip_suffix(".md").unwrap_or(inside).to_string());
    }
    let in_docs = config
        .and_then(|config| config.doc_root_of(&path))
        .is_some_and(|folder| folder != path);
    in_docs.then_some(path)
}

/// `link_id` for a link in a store item, which can only name other store items.
pub(crate) fn join_target(from_id: &str, target: &str) -> Option<String> {
    link_id(None, &format!("stone/{from_id}.md"), target)
}
