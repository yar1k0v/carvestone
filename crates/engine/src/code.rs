//! `stone code find|uses|deps`: SQL over the symbols and edges that refresh
//! keeps. One serde result per query, shared by the CLI `--json`, MCP and HTTP.

use std::collections::BTreeMap;

use carvestone_index::Index;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::code_gap;
use crate::code_index::building_note;
use crate::code_rank::rank;
use crate::refresh::{BATCH_FILES, refresh_in};
use crate::repo::{EngineError, Repo};
use crate::search::Hit;

const DEFAULT_LIMIT: usize = 20;
/// Candidates an ambiguity error lists.
const CANDIDATES_SHOWN: usize = 10;
/// Rows `find` reads before the limit and budget.
const FIND_ROWS: i64 = 1000;

/// One code query. Every operation reads the fields it needs and ignores the rest.
#[derive(Debug, Clone, Default)]
pub struct CodeQuery {
    pub target: String,
    /// The second end of `path`.
    pub to: Option<String>,
    /// `impact` depth; default 3.
    pub depth: Option<usize>,
    /// `find`: only symbols of this kind.
    pub kind: Option<String>,
    /// Drop inferred edges.
    pub exact_only: bool,
    pub limit: usize,
    /// Stops adding hits once their printed lines would cost more than this
    /// (chars / 4); at least one hit is always returned.
    pub budget: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeResult {
    pub query: String,
    pub hits: Vec<Hit>,
    /// The limit or the budget cut hits off.
    pub truncated: bool,
    /// How many hits the limit or the budget left out.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// "code index is still building ..." while the fill is unfinished.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Set only on an empty answer: what is indexed, what is not, and the grep command to run next.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<String>,
}

/// What a target resolved to: the symbol rows sharing one qualified name, or
/// all symbols of one file.
pub(crate) struct Target {
    pub label: String,
    pub ids: Vec<i64>,
    /// Set for a file target, so edges inside the file are not its uses.
    pub file_id: Option<i64>,
}

/// The members of the types among `ids`: what a static class is used through, since a static call
/// never names the class in an edge of its own.
fn member_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>, EngineError> {
    let mut out = Vec::new();
    for id in ids {
        out.extend(ids_where(
            conn,
            "SELECT m.id FROM symbols m JOIN symbols t ON t.id = m.parent_id \
             WHERE t.id = ?1 AND t.kind = 'type' ORDER BY m.id",
            id,
        )?);
    }
    Ok(out)
}

/// The index, brought up to date first like `search` does.
pub(crate) fn open(repo: &Repo) -> Result<Index, EngineError> {
    let index = repo.index()?;
    refresh_in(repo, &index, BATCH_FILES, None)?;
    Ok(index)
}

pub(crate) fn effective_limit(limit: usize) -> usize {
    if limit == 0 { DEFAULT_LIMIT } else { limit }
}

/// A hit for a symbol row. A file symbol is named by its path.
pub(crate) fn symbol_hit(
    qname: &str,
    kind: &str,
    path: &str,
    lines: [u32; 2],
    exactness: &str,
    snippet: String,
) -> Hit {
    Hit {
        id: qname.to_string(),
        anchor: None,
        path: path.to_string(),
        lines,
        kind: kind.to_string(),
        snippet,
        exactness: Some(exactness.to_string()),
    }
}

pub(crate) fn exactness(exact: bool) -> &'static str {
    if exact { "exact" } else { "inferred" }
}

/// Keeps hits while the limit and the budget allow; the first hit always stays. Returns the kept
/// hits and how many were left out.
pub(crate) fn cut(hits: Vec<Hit>, limit: usize, budget: Option<usize>) -> (Vec<Hit>, usize) {
    let limit = effective_limit(limit);
    let total = hits.len();
    let mut kept = Vec::new();
    let mut spent = 0;
    for hit in hits {
        let cost = hit.cost();
        if kept.len() == limit || budget.is_some_and(|b| !kept.is_empty() && spent + cost > b) {
            break;
        }
        spent += cost;
        kept.push(hit);
    }
    let omitted = total - kept.len();
    (kept, omitted)
}

/// Serde skips a count of zero.
pub(crate) fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn file_id_of(conn: &Connection, path: &str) -> Result<Option<i64>, EngineError> {
    Ok(conn
        .query_row(
            "SELECT id FROM files WHERE path = ?1 AND lang IS NOT NULL",
            [path],
            |row| row.get(0),
        )
        .optional()?)
}

fn ids_where(conn: &Connection, sql: &str, arg: impl rusqlite::ToSql) -> Result<Vec<i64>, EngineError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([arg], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn file_target(conn: &Connection, path: &str, file_id: i64) -> Result<Target, EngineError> {
    Ok(Target {
        label: path.to_string(),
        ids: ids_where(conn, "SELECT id FROM symbols WHERE file_id = ?1 ORDER BY id", file_id)?,
        file_id: Some(file_id),
    })
}

/// A qualified name, a unique plain name, a path, or `path:line`.
pub(crate) fn resolve_target(conn: &Connection, input: &str) -> Result<Target, EngineError> {
    let text = input.trim().replace('\\', "/");
    if text.is_empty() {
        return Err(EngineError::Invalid(
            "give a symbol, a file path or path:line".to_string(),
        ));
    }
    if let Some(file_id) = file_id_of(conn, &text)? {
        return file_target(conn, &text, file_id);
    }
    if let Some((path, line)) = text.rsplit_once(':')
        && let Ok(line) = line.parse::<u32>()
        && let Some(file_id) = file_id_of(conn, path)?
    {
        let inner: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, qname FROM symbols WHERE file_id = ?1 AND kind <> 'file' \
                 AND line_start <= ?2 AND line_end >= ?2 ORDER BY line_end - line_start, id LIMIT 1",
                params![file_id, line],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        return match inner {
            Some((id, qname)) => Ok(Target {
                label: qname,
                ids: vec![id],
                file_id: None,
            }),
            None => file_target(conn, path, file_id),
        };
    }
    let by_qname = ids_where(conn, "SELECT id FROM symbols WHERE qname = ?1 ORDER BY id", &text)?;
    if !by_qname.is_empty() {
        return Ok(Target {
            label: text,
            ids: by_qname,
            file_id: None,
        });
    }
    let mut stmt =
        conn.prepare("SELECT DISTINCT qname FROM symbols WHERE name = ?1 AND kind != 'file' ORDER BY qname")?;
    let names: Vec<String> = stmt.query_map([&text], |row| row.get(0))?.collect::<Result<_, _>>()?;
    match names.as_slice() {
        [] => Err(EngineError::Invalid(format!(
            "no symbol or file matches `{text}`; try `stone code find {text}`"
        ))),
        [qname] => Ok(Target {
            ids: ids_where(conn, "SELECT id FROM symbols WHERE qname = ?1 ORDER BY id", qname)?,
            label: qname.clone(),
            file_id: None,
        }),
        many => {
            let shown: Vec<&str> = many.iter().take(CANDIDATES_SHOWN).map(String::as_str).collect();
            let more = if many.len() > CANDIDATES_SHOWN { ", ..." } else { "" };
            Err(EngineError::Invalid(format!(
                "{text} matches {} symbols; use one of: {}{more}",
                many.len(),
                shown.join(", ")
            )))
        }
    }
}

fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn result(conn: &Connection, q: &CodeQuery, query: String, hits: Vec<Hit>) -> Result<CodeResult, EngineError> {
    let (hits, omitted) = cut(hits, q.limit, q.budget);
    Ok(CodeResult {
        query,
        hits,
        truncated: omitted > 0,
        omitted,
        note: building_note(conn)?,
        gap: None,
    })
}

/// What a `find` with no hits says when the store names no code root, so an empty answer is not read as "no such symbol".
const NO_ROOTS_NOTE: &str =
    "no code is indexed, because stone/config.yml has no code_roots; add code_roots: [.] to index the whole repo";

/// Symbols by qualified name, exact name or prefix of either.
pub fn find(repo: &Repo, q: &CodeQuery) -> Result<CodeResult, EngineError> {
    let text = q.target.trim();
    if text.is_empty() {
        return Err(EngineError::Invalid("give a name or a prefix to find".to_string()));
    }
    let kind = q.kind.as_deref().map(find_kind).transpose()?;
    let index = open(repo)?;
    let conn = index.conn();
    let like = format!("{}%", escape_like(text));
    let mut stmt = conn.prepare(
        "SELECT s.qname, s.kind, s.signature, s.line_start, s.line_end, f.path, \
                CASE WHEN s.qname = ?1 THEN 0 WHEN s.name = ?1 THEN 1 ELSE 2 END \
         FROM symbols s JOIN files f ON f.id = s.file_id \
         WHERE (s.qname = ?1 OR s.name = ?1 OR s.qname LIKE ?2 ESCAPE '\\' OR s.name LIKE ?2 ESCAPE '\\') \
           AND (?3 IS NULL OR s.kind = ?3) AND (?3 = 'file' OR s.kind != 'file') \
         ORDER BY 7, s.qname, f.path, s.line_start LIMIT ?4",
    )?;
    let hits: Vec<Hit> = stmt
        .query_map(params![text, like, kind, FIND_ROWS], |row| {
            let qname: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let path: String = row.get(5)?;
            Ok(symbol_hit(
                &qname,
                &kind,
                &path,
                [row.get(3)?, row.get(4)?],
                "exact",
                row.get(2)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut found = result(conn, q, text.to_string(), hits)?;
    if found.hits.is_empty() {
        found.gap = Some(code_gap::gap(conn, code_gap::Asked::Store, &[text])?);
    }
    // A building note wins: the index may fill in soon.
    if found.hits.is_empty() && found.note.is_none() && repo.store.config.code_roots.is_empty() {
        found.note = Some(NO_ROOTS_NOTE.to_string());
    }
    Ok(found)
}

/// The stored kind for a `--kind` word: a stored kind as is, the words of the languages for
/// their stored kind (`class` is a `type`, `field` a `member`), else an error naming the kinds.
fn find_kind(word: &str) -> Result<&'static str, EngineError> {
    const KINDS: [&str; 7] = ["type", "method", "function", "member", "module", "file", "project"];
    let word = word.trim().to_ascii_lowercase();
    if let Some(kind) = KINDS.iter().find(|kind| **kind == word) {
        return Ok(kind);
    }
    match word.as_str() {
        "class" | "interface" | "record" | "struct" | "enum" | "trait" | "delegate" => Ok("type"),
        "field" | "variable" | "const" | "constant" => Ok("member"),
        "constructor" | "property" => Ok("method"),
        "fn" | "func" => Ok("function"),
        "namespace" | "package" | "mod" => Ok("module"),
        _ => Err(EngineError::Invalid(format!(
            "no symbol kind `{word}`; use one of: {} (class, interface, record, struct and enum are type)",
            KINDS.join(", ")
        ))),
    }
}

/// Incoming `calls`, `imports`, `inherits`, `implements`, `references` and `dispatch` edges.
/// Inferred ones come after exact ones, the likelier first (see `code_rank`). A type also lists
/// what reaches its members from outside it, so a static class shows the code that calls it.
pub fn uses(repo: &Repo, q: &CodeQuery) -> Result<CodeResult, EngineError> {
    edge_query(repo, q, true)
}

/// Outgoing edges: what the symbol or file imports and calls.
pub fn deps(repo: &Repo, q: &CodeQuery) -> Result<CodeResult, EngineError> {
    edge_query(repo, q, false)
}

struct Found {
    exact: bool,
    line: u32,
    /// How likely an inferred edge is real; unused for an exact one.
    rank: (bool, usize),
    hit: Hit,
}

fn edge_query(repo: &Repo, q: &CodeQuery, incoming: bool) -> Result<CodeResult, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let target = resolve_target(conn, &q.target)?;
    // `self_col` is the end the target sits on, `other_col` the end listed.
    let (self_col, other_col) = if incoming {
        ("dst_id", "src_id")
    } else {
        ("src_id", "dst_id")
    };
    let sql = format!(
        "SELECT o.id, o.qname, o.kind, o.signature, o.line_start, o.line_end, f.path, e.kind, e.exact, e.line, tf.path, sf.path \
         FROM edge_ends e JOIN symbols o ON o.id = e.{other_col} JOIN files f ON f.id = o.file_id \
         JOIN symbols t ON t.id = e.{self_col} JOIN files tf ON tf.id = t.file_id \
         JOIN files sf ON sf.id = e.file_id \
         WHERE e.{self_col} = ?1 AND (?2 IS NULL OR o.file_id != ?2) AND (?3 = 0 OR e.exact = 1)"
    );
    let mut stmt = conn.prepare(&sql)?;
    // One entry per other end and edge kind: the exact edge wins, then the first site.
    let mut found: BTreeMap<(i64, String), Found> = BTreeMap::new();
    let members = if incoming && target.file_id.is_none() {
        member_ids(conn, &target.ids)?
    } else {
        Vec::new()
    };
    let inside = format!("{}.", target.label);
    let own = target.ids.iter().map(|id| (id, false));
    for (id, member) in own.chain(members.iter().map(|id| (id, true))) {
        let rows = stmt.query_map(params![id, target.file_id, q.exact_only], |row| {
            let other: i64 = row.get(0)?;
            let qname: String = row.get(1)?;
            let signature: String = row.get(3)?;
            let path: String = row.get(6)?;
            let edge_kind: String = row.get(7)?;
            let exact = row.get::<_, i64>(8)? != 0;
            let line: u32 = row.get(9)?;
            let self_path: String = row.get(10)?;
            // A site written in another file than the edge's source (a dispatch edge is stored in
            // the implementer's file) names that file, so the line reads against the file it is in.
            let site_path: String = row.get(11)?;
            let source_path = if incoming { &path } else { &self_path };
            let at = if site_path == *source_path {
                format!("@L{line}")
            } else {
                format!("@{site_path}:L{line}")
            };
            let snippet = format!("{at} {signature}").trim_end().to_string();
            Ok((
                (other, edge_kind.clone()),
                Found {
                    exact,
                    line,
                    rank: if exact { (false, 0) } else { rank(&path, &self_path) },
                    hit: symbol_hit(
                        &qname,
                        &edge_kind,
                        &path,
                        [row.get(4)?, row.get(5)?],
                        exactness(exact),
                        snippet,
                    ),
                },
            ))
        })?;
        for row in rows {
            let (key, entry) = row?;
            // A type's own code calling its members is no use of the type.
            if member && (entry.hit.id == target.label || entry.hit.id.starts_with(&inside)) {
                continue;
            }
            match found.get(&key) {
                Some(old)
                    if (old.exact, std::cmp::Reverse(old.line)) >= (entry.exact, std::cmp::Reverse(entry.line)) => {}
                _ => {
                    found.insert(key, entry);
                }
            }
        }
    }
    let mut ordered: Vec<Found> = found.into_values().collect();
    ordered.sort_by(|a, b| {
        b.exact
            .cmp(&a.exact)
            .then_with(|| b.rank.cmp(&a.rank))
            .then_with(|| a.hit.path.cmp(&b.hit.path))
            .then_with(|| a.hit.lines[0].cmp(&b.hit.lines[0]))
            .then_with(|| a.hit.id.cmp(&b.hit.id))
            .then_with(|| a.line.cmp(&b.line))
    });
    let hits: Vec<Hit> = ordered.into_iter().map(|f| f.hit).collect();
    // The gap reads the target, so it is built before the label moves into the answer.
    let gap = if hits.is_empty() {
        Some(code_gap::of_target(conn, &target)?)
    } else {
        None
    };
    let mut answer = result(conn, q, target.label, hits)?;
    answer.gap = gap;
    Ok(answer)
}

/// One declaration of an outline, `depth` levels inside the outermost one shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OutlineEntry {
    pub qname: String,
    pub name: String,
    pub kind: String,
    /// The header as written, whitespace collapsed.
    pub signature: String,
    /// The header at a glance: no modifiers, one word per parameter, long generics folded.
    pub short: String,
    pub lines: [u32; 2],
    pub depth: u32,
}

impl OutlineEntry {
    /// The printed row of the short outline: indent by depth, kind, header, then the lines.
    pub fn line(&self) -> String {
        let lines = if self.lines[0] == self.lines[1] {
            format!("L{}", self.lines[0])
        } else {
            format!("L{}-{}", self.lines[0], self.lines[1])
        };
        let indent = "  ".repeat(self.depth as usize + 1);
        format!("{indent}{} {}  {lines}", self.kind, self.short)
    }
}

/// The declarations of a file, or of one symbol and what it holds, in line order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Outline {
    pub query: String,
    pub path: String,
    pub entries: Vec<OutlineEntry>,
    /// How many entries the limit left out.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Set only on an empty answer: what is indexed, what is not, and the grep command to run next.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<String>,
}

/// The outline of a file (every declaration in it) or of a symbol (it and everything inside it),
/// nested by the declaration that holds each one. A name several files declare is refused; name
/// the file instead.
pub fn outline(repo: &Repo, q: &CodeQuery) -> Result<Outline, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let target = resolve_target(conn, &q.target)?;
    let file_id = match target.file_id {
        Some(file_id) => file_id,
        None => {
            let mut stmt = conn.prepare("SELECT DISTINCT file_id FROM symbols WHERE id = ?1")?;
            let mut files = std::collections::BTreeSet::new();
            for id in &target.ids {
                files.extend(
                    stmt.query_map([id], |row| row.get::<_, i64>(0))?
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            match files.into_iter().collect::<Vec<_>>().as_slice() {
                [one] => *one,
                _ => {
                    return Err(EngineError::Invalid(format!(
                        "{} is declared in several files; outline one of them by its path",
                        target.label
                    )));
                }
            }
        }
    };
    let path: String = conn.query_row("SELECT path FROM files WHERE id = ?1", [file_id], |row| row.get(0))?;
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, qname, name, kind, signature, line_start, line_end FROM symbols \
         WHERE file_id = ?1 AND kind != 'file' ORDER BY line_start, line_end DESC, id",
    )?;
    let rows: Vec<(i64, Option<i64>, OutlineEntry)> = stmt
        .query_map([file_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                OutlineEntry {
                    qname: row.get(2)?,
                    name: row.get(3)?,
                    kind: row.get(4)?,
                    signature: row
                        .get::<_, String>(5)?
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    short: String::new(),
                    lines: [row.get(6)?, row.get(7)?],
                    depth: 0,
                },
            ))
        })?
        .collect::<Result<_, _>>()?;
    let rows = rows.into_iter().map(|(id, parent, mut entry)| {
        entry.short = crate::code_short::short_signature(&entry.signature, &entry.name, &entry.kind);
        (id, parent, entry)
    });
    // A parent comes before its children in line order, so one pass knows every depth.
    let mut depth: BTreeMap<i64, u32> = BTreeMap::new();
    let mut entries = Vec::new();
    for (id, parent, mut entry) in rows {
        let inside = parent.and_then(|p| depth.get(&p).copied());
        let shown = if target.file_id.is_some() {
            Some(inside.map_or(0, |d| d + 1))
        } else if target.ids.contains(&id) {
            Some(0)
        } else {
            inside.map(|d| d + 1)
        };
        let Some(level) = shown else { continue };
        depth.insert(id, level);
        entry.depth = level;
        entries.push(entry);
    }
    let limit = if q.limit == 0 { usize::MAX } else { q.limit };
    let omitted = entries.len().saturating_sub(limit);
    entries.truncate(limit);
    // Built before the target moves into the answer.
    let gap = if entries.is_empty() {
        Some(code_gap::of_target(conn, &target)?)
    } else {
        None
    };
    Ok(Outline {
        query: target.label,
        path,
        entries,
        omitted,
        note: building_note(conn)?,
        gap,
    })
}
