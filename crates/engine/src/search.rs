//! Section search over the derived index: FTS5 candidates, BM25 weighted by
//! title, heading and body, an optional boost for items linked to `from`, and
//! a token budget on the printed hit lines.

use std::collections::HashSet;

use carvestone_core::IdScheme;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::refresh::{BATCH_FILES, refresh_in};
use crate::repo::{EngineError, Repo};
use crate::tiers;

/// Candidates read from the index before boosting and the budget.
const CANDIDATES: i64 = 1000;
const SYMBOL_CANDIDATES: i64 = 50;
const FROM_BOOST: f64 = 1.5;
const COMPONENT_BOOST: f64 = 1.3;
const SNIPPET_CHARS: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    /// Sections of items linked to this one (either direction) rank ahead.
    pub from: Option<String>,
    /// Only items that list this component or live under it.
    pub component: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub milestone: Option<String>,
    pub limit: usize,
    /// Stops adding hits once their printed lines would cost more than this
    /// (chars / 4); at least one hit is always returned.
    pub budget: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Hit {
    pub id: String,
    pub anchor: Option<String>,
    pub path: String,
    pub lines: [u32; 2],
    pub kind: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub exactness: Option<String>,
}

impl Hit {
    /// `<id>[#anchor]  L<start>-<end>  <kind>  <snippet>`, the line CLI, MCP and HTTP print.
    pub fn line(&self) -> String {
        // Code hits carry exactness: `<qname|path>  <path>:L<a>-<b>  <kind>  <exact|inferred>  <detail>`.
        if let Some(exactness) = &self.exactness {
            let line = format!(
                "{}  {}:L{}-{}  {}  {exactness}  {}",
                self.id, self.path, self.lines[0], self.lines[1], self.kind, self.snippet
            );
            return line.trim_end().to_string();
        }
        let anchor = self.anchor.as_ref().map_or(String::new(), |a| format!("#{a}"));
        format!(
            "{}{anchor}  L{}-{}  {}  {}",
            self.id, self.lines[0], self.lines[1], self.kind, self.snippet
        )
    }

    /// Tokens the printed line costs an agent.
    pub fn cost(&self) -> usize {
        self.line().chars().count().div_ceil(4)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SearchResult {
    pub query: String,
    pub hits: Vec<Hit>,
    /// The limit or the budget cut hits off.
    pub truncated: bool,
    /// How many hits the limit or the budget left out.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub omitted: usize,
    /// The size of the hits the limit or the budget left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left_out: Option<tiers::LeftOut>,
    /// The index returned its full candidate cap, so more hits may exist that were never read.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub uncounted: bool,
}

struct Candidate {
    hit: Hit,
    item_id: String,
    milestone: Option<String>,
    rank: f64,
    title: String,
    text: String,
}

pub fn search(repo: &Repo, q: &SearchQuery) -> Result<SearchResult, EngineError> {
    let words = words_of(&q.text);
    if words.is_empty() {
        return Err(EngineError::EmptyQuery);
    }
    let scheme = repo.store.config.scheme();
    let index = repo.index()?;
    refresh_in(repo, &index, BATCH_FILES, None)?;
    let conn = index.conn();

    let from = match &q.from {
        Some(input) => Some(resolve_from(conn, &scheme, input)?),
        None => None,
    };
    let linked: HashSet<String> = match &from {
        Some(id) => linked_items(conn, id)?,
        None => HashSet::new(),
    };
    let component = q.component.as_deref().map(|c| canonical(&scheme, c));
    let in_component: HashSet<String> = match &component {
        Some(c) => component_items(conn, c)?,
        None => HashSet::new(),
    };
    let milestone = q.milestone.as_deref().map(|m| canonical(&scheme, m));

    let mut found = candidates(conn, &fts_and(&words), q)?;
    if found.is_empty() && words.len() > 1 {
        found = candidates(conn, &fts_or(&words), q)?;
    }
    // Read before the filters below: a full cap means the index holds more than was read.
    let mut uncounted = found.len() as i64 >= CANDIDATES;
    let in_comp = |item_id: &str| {
        component.as_ref().is_some_and(|comp| {
            in_component.contains(item_id) || item_id == comp || item_id.starts_with(&format!("{comp}/"))
        })
    };
    found.retain(|c| {
        (component.is_none() || in_comp(&c.item_id))
            && milestone
                .as_ref()
                .is_none_or(|m| c.milestone.as_deref().map(|v| canonical(&scheme, v)).as_ref() == Some(m))
    });
    // BM25 scores are negative, so a larger factor ranks a hit further ahead.
    for c in &mut found {
        if linked.contains(&c.item_id) {
            c.rank *= FROM_BOOST;
        }
        if in_comp(&c.item_id) {
            c.rank *= COMPONENT_BOOST;
        }
    }
    found.sort_by(|a, b| {
        a.rank
            .total_cmp(&b.rank)
            .then_with(|| a.item_id.cmp(&b.item_id))
            .then_with(|| a.hit.lines[0].cmp(&b.hit.lines[0]))
    });

    // Symbols named like the query lead, the rest follow the sections.
    let symbols = symbol_hits(conn, &words, q)?;
    uncounted |= symbols.len() as i64 >= SYMBOL_CANDIDATES;
    let (named, other_symbols): (Vec<_>, Vec<_>) = symbols.into_iter().partition(|(named, _)| *named);
    let ordered: Vec<Found> = named
        .into_iter()
        .map(|(_, hit)| Found::Symbol(hit))
        .chain(found.into_iter().map(Found::Section))
        .chain(other_symbols.into_iter().map(|(_, hit)| Found::Symbol(hit)))
        .collect();
    let total = ordered.len();

    let limit = if q.limit == 0 { 10 } else { q.limit };
    let mut hits = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    let mut spent = 0;
    let mut stopped = false;
    // Snippets come from the candidate rows, so a hit past the limit costs no query.
    for item in ordered {
        let hit = match item {
            Found::Symbol(hit) => hit,
            Found::Section(c) => {
                let mut hit = c.hit;
                hit.snippet = snippet(&c.title, &c.text);
                hit
            }
        };
        let cost = hit.cost();
        if !stopped && (hits.len() == limit || q.budget.is_some_and(|b| !hits.is_empty() && spent + cost > b)) {
            stopped = true;
        }
        if stopped {
            dropped.push(hit.line());
            continue;
        }
        spent += cost;
        hits.push(hit);
    }
    let omitted = total - hits.len();
    Ok(SearchResult {
        query: q.text.clone(),
        hits,
        truncated: omitted > 0,
        omitted,
        left_out: tiers::left_out("hits", &dropped),
        uncounted,
    })
}

enum Found {
    Section(Candidate),
    Symbol(Hit),
}

/// Symbols matching every word, best first, each flagged when its name or qualified name is the
/// whole query. They only answer queries that no section filter narrows.
fn symbol_hits(conn: &Connection, words: &[String], q: &SearchQuery) -> Result<Vec<(bool, Hit)>, EngineError> {
    if q.status.is_some()
        || q.milestone.is_some()
        || q.component.is_some()
        || q.kind.as_deref().is_some_and(|kind| kind != "symbol")
    {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT s.qname, s.name, s.signature, s.line_start, s.line_end, f.path, \
                bm25(symbols_fts, 10.0, 5.0, 2.0, 1.0) \
         FROM symbols_fts JOIN symbols s ON s.id = symbols_fts.rowid JOIN files f ON f.id = s.file_id \
         WHERE symbols_fts MATCH ?1 AND s.kind NOT IN ('file', 'project') \
         ORDER BY 7, s.qname, f.path, s.line_start LIMIT ?2",
    )?;
    let whole = q.text.trim().to_lowercase();
    let rows = stmt.query_map(params![fts_and(words), SYMBOL_CANDIDATES], |row| {
        let qname: String = row.get(0)?;
        let name: String = row.get(1)?;
        let named = name.to_lowercase() == whole || qname.to_lowercase() == whole;
        Ok((
            named,
            Hit {
                id: qname,
                anchor: None,
                path: row.get(5)?,
                lines: [row.get(3)?, row.get(4)?],
                kind: "symbol".to_string(),
                snippet: row.get(2)?,
                exactness: None,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Words the FTS table can match: the tokenizer keeps `-` and `_` inside words.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .map(|w| w.trim_matches('-'))
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .map(str::to_string)
        .collect()
}

/// Every word quoted, the last one also as a prefix.
fn fts_terms(words: &[String]) -> Vec<String> {
    let last = words.len() - 1;
    words
        .iter()
        .enumerate()
        .map(|(i, w)| format!("\"{w}\"{}", if i == last { "*" } else { "" }))
        .collect()
}

fn fts_and(words: &[String]) -> String {
    fts_terms(words).join(" AND ")
}

fn fts_or(words: &[String]) -> String {
    fts_terms(words).join(" OR ")
}

fn candidates(conn: &Connection, fts: &str, q: &SearchQuery) -> Result<Vec<Candidate>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT s.item_id, s.anchor, f.path, s.line_start, s.line_end, s.kind, s.milestone, s.title, s.text, \
                bm25(fts, 10.0, 5.0, 1.0) \
         FROM fts JOIN sections s ON s.id = fts.rowid JOIN files f ON f.id = s.file_id \
         WHERE fts MATCH ?1 AND (?2 IS NULL OR s.kind = ?2) AND (?3 IS NULL OR s.status = ?3) \
         ORDER BY 10 LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![fts, q.kind, q.status, CANDIDATES], |row| {
        let item_id: String = row.get(0)?;
        let anchor: String = row.get(1)?;
        Ok(Candidate {
            hit: Hit {
                id: item_id.clone(),
                anchor: (!anchor.is_empty()).then_some(anchor),
                path: row.get(2)?,
                lines: [row.get(3)?, row.get(4)?],
                kind: row.get(5)?,
                snippet: String::new(),
                exactness: None,
            },
            item_id,
            milestone: row.get(6)?,
            rank: row.get(9)?,
            title: row.get(7)?,
            text: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The section text without its heading line, whitespace collapsed, cut to
/// `SNIPPET_CHARS`; the title when the section has no other text.
fn snippet(title: &str, text: &str) -> String {
    let body = text
        .lines()
        .skip_while(|l| l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = if flat.is_empty() { title.to_string() } else { flat };
    if flat.chars().count() > SNIPPET_CHARS {
        let cut: String = flat.chars().take(SNIPPET_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    }
}

fn canonical(scheme: &IdScheme, text: &str) -> String {
    match scheme.parse(text) {
        Ok(r) => r.id.as_str().to_string(),
        Err(_) => text.to_lowercase(),
    }
}

fn resolve_from(conn: &Connection, scheme: &IdScheme, input: &str) -> Result<String, EngineError> {
    let not_found = || EngineError::NotFound {
        id: input.to_string(),
        suggestion: None,
    };
    let id = scheme.parse(input).map_err(|_| not_found())?.id;
    let known: Option<i64> = conn
        .query_row("SELECT 1 FROM files WHERE item_id = ?1", [id.as_str()], |r| r.get(0))
        .optional()?;
    known.map(|_| id.as_str().to_string()).ok_or_else(not_found)
}

fn linked_items(conn: &Connection, id: &str) -> Result<HashSet<String>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT dst_item FROM links WHERE src_item = ?1 UNION SELECT src_item FROM links WHERE dst_item = ?1",
    )?;
    let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn component_items(conn: &Connection, component: &str) -> Result<HashSet<String>, EngineError> {
    let mut stmt = conn.prepare("SELECT src_item FROM links WHERE kind = 'components' AND dst_item = ?1")?;
    let rows = stmt.query_map([component], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}
