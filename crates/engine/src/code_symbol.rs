//! `GET /api/code/symbol`: a class page in one answer: what the symbol is, its doc comment, its
//! source text, its members, and what uses it and what it uses, each link naming the members it
//! goes through. Links between a symbol's own members are left out.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::code::{CodeQuery, exactness, open, resolve_target, symbol_hit};
use crate::code_index::building_note;
use crate::code_map::{CodeMap, Link, Sym, code_map, plain_doc};
use crate::code_short::short_signature;
use crate::repo::{EngineError, Repo};
use crate::search::Hit;

/// Source lines served at most; a longer symbol says so.
const MAX_SOURCE_LINES: usize = 2000;
/// Kinds a member of a page may be.
const NOT_MEMBERS: [&str; 1] = ["file"];

/// The source text of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SymbolSource {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
    /// The text stops before `end_line`.
    pub truncated: bool,
}

/// One declaration inside a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SymbolMember {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub short: String,
    pub lines: [u32; 2],
    /// How far it sits inside the symbol; 0 is directly inside.
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub doc: Option<String>,
    /// How many symbols outside this one use it.
    pub used_by: usize,
    /// How many symbols outside this one it uses.
    pub uses: usize,
}

/// A symbol on the other end of a link, and which members of this symbol it links to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SymbolEdge {
    pub hit: Hit,
    pub through: Vec<String>,
}

/// What `GET /api/code/symbol` answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SymbolPage {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub signature: String,
    /// The doc comment above it, markers removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub source: Option<SymbolSource>,
    /// It sits in a file with no language, whose text is kept back on purpose.
    pub source_private: bool,
    pub members: Vec<SymbolMember>,
    pub used_by: Vec<SymbolEdge>,
    pub uses: Vec<SymbolEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
}

/// The lines `start..=end` of a text, the line ends as written.
fn slice(text: &str, start: u32, end: u32) -> (String, bool) {
    let wanted = (end.saturating_sub(start) as usize).saturating_add(1);
    let shown = wanted.min(MAX_SOURCE_LINES);
    let lines: String = text
        .split_inclusive('\n')
        .skip((start as usize).saturating_sub(1))
        .take(shown)
        .collect();
    (lines, wanted > shown)
}

fn is_private(map: &CodeMap, sym: &Sym) -> bool {
    !map.langs.contains_key(&sym.file)
}

/// The text of a symbol. A file with no language is kept back.
fn source_of(repo: &Repo, map: &CodeMap, sym: &Sym) -> Option<SymbolSource> {
    if is_private(map, sym) {
        return None;
    }
    let file = map.path(sym).to_string();
    let bytes = std::fs::read(repo.store.layout.root.join(&file)).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let (text, truncated) = slice(&text, sym.line_start, sym.line_end);
    Some(SymbolSource {
        file,
        start_line: sym.line_start,
        end_line: sym.line_end,
        text,
        truncated,
    })
}

/// `id` and everything inside it, each with how deep it sits; the symbol itself has depth zero.
fn inner(map: &CodeMap, id: i64) -> Vec<(i64, u32)> {
    let mut found = vec![(id, 0)];
    let mut at = 0;
    while at < found.len() {
        let (current, depth) = found[at];
        found.extend(map.inside(current).map(|member| (member.id, depth + 1)));
        at += 1;
    }
    found
}

/// One row per symbol and edge kind on the far end of `links`, the exact link winning.
#[derive(Default)]
struct Rows(BTreeMap<(i64, String), (SymbolEdge, bool)>);

impl Rows {
    fn add(&mut self, map: &CodeMap, link: &Link, member: Option<&str>) {
        let Some(other) = map.syms.get(&link.other) else {
            return;
        };
        let key = (other.id, link.kind.clone());
        let hit = || {
            symbol_hit(
                &other.qname,
                &link.kind,
                map.path(other),
                [other.line_start, other.line_end],
                exactness(link.exact),
                format!("@L{} {}", link.line, other.signature).trim_end().to_string(),
            )
        };
        let row = self.0.entry(key).or_insert_with(|| {
            (
                SymbolEdge {
                    hit: hit(),
                    through: Vec::new(),
                },
                link.exact,
            )
        });
        if link.exact && !row.1 {
            row.0.hit = hit();
            row.1 = true;
        }
        if let Some(member) = member
            && !row.0.through.iter().any(|name| name == member)
        {
            row.0.through.push(member.to_string());
        }
    }

    /// Exact links first, then by name.
    fn sorted(self) -> Vec<SymbolEdge> {
        let mut rows: Vec<(SymbolEdge, bool)> = self.0.into_values().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.hit.id.cmp(&b.0.hit.id)));
        rows.into_iter().map(|(edge, _)| edge).collect()
    }
}

/// The page of a class, function, or any other symbol `q.target` names. The first part of a
/// partial class gives the header and the source; the members and links are those of all parts.
pub fn symbol(repo: &Repo, q: &CodeQuery) -> Result<SymbolPage, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let target = resolve_target(conn, &q.target)?;
    let map = code_map(repo, conn)?;
    let Some(first) = target.ids.iter().find_map(|id| map.syms.get(id)) else {
        return Err(EngineError::Invalid(format!(
            "`{}` has no declaration; try `stone code find {}`",
            q.target, q.target
        )));
    };

    let mut own: Vec<(i64, u32)> = Vec::new();
    for id in &target.ids {
        own.extend(inner(&map, *id));
    }
    let own_ids: HashSet<i64> = own.iter().map(|(id, _)| *id).collect();

    let mut used_by = Rows::default();
    let mut uses = Rows::default();
    let mut members = Vec::new();
    for (id, depth) in &own {
        let Some(sym) = map.syms.get(id) else { continue };
        let is_member = *depth > 0;
        let shown = is_member.then_some(sym.name.as_str());
        let incoming: Vec<&Link> = map
            .links_to(*id)
            .iter()
            .filter(|l| !own_ids.contains(&l.other))
            .collect();
        let outgoing: Vec<&Link> = map.out_of(*id).iter().filter(|l| !own_ids.contains(&l.other)).collect();
        for link in &incoming {
            used_by.add(&map, link, shown);
        }
        for link in &outgoing {
            uses.add(&map, link, shown);
        }
        if is_member && !NOT_MEMBERS.contains(&sym.kind.as_str()) {
            let distinct = |links: &[&Link]| links.iter().map(|l| l.other).collect::<HashSet<_>>().len();
            let signature = sym.signature.lines().next().unwrap_or_default().trim().to_string();
            members.push(SymbolMember {
                id: sym.qname.clone(),
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                short: short_signature(&signature, &sym.name, &sym.kind),
                signature,
                lines: [sym.line_start, sym.line_end],
                depth: depth - 1,
                doc: sym.doc.as_deref().map(plain_doc),
                used_by: distinct(&incoming),
                uses: distinct(&outgoing),
            });
        }
    }
    members.sort_by(|a, b| (a.lines, &a.id).cmp(&(b.lines, &b.id)));

    Ok(SymbolPage {
        id: first.qname.clone(),
        name: first.name.clone(),
        kind: first.kind.clone(),
        signature: first.signature.clone(),
        doc: first.doc.as_deref().map(plain_doc),
        source: source_of(repo, &map, first),
        source_private: is_private(&map, first),
        members,
        used_by: used_by.sorted(),
        uses: uses.sorted(),
        note: building_note(conn)?,
    })
}
