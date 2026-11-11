//! Reading a SCIP index into plain data. SCIP is the symbol index the language
//! SDKs' indexers write (rust-analyzer, scip-typescript, scip-dotnet,
//! scip-python). Only what the native pass needs is kept: per document, the
//! occurrences of global symbols with their single-line ranges, and the
//! `is_implementation` relationships. Nothing here touches the filesystem or
//! the index; the engine maps the result onto declarations.
//!
//! Where SCIP and our facts differ:
//! - SCIP positions are 0-based and count characters in the document's
//!   position encoding; ours are 1-based lines with no columns. The engine
//!   joins on the line and on the identifier text the range covers, which
//!   [`slice_line`] cuts out for any of the three encodings.
//! - A SCIP symbol is a global string; ours are qualified names. They are
//!   joined through the definition occurrences, never by comparing strings.
//! - Local symbols (`local 12`) live inside one document and name no
//!   declaration of ours, so they are dropped here.
//! - Ranges across several lines (a whole call expression) are not identifiers
//!   and are dropped.

use protobuf::Message;
use scip::types::{
    Index, PositionEncoding, SymbolRole,
    occurrence::{Typed_enclosing_range, Typed_range},
};

/// How a document counts the characters of a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

/// One occurrence of a global symbol on a single line. Lines and columns are 0-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipOccurrence {
    pub symbol: String,
    pub line: u32,
    pub start: u32,
    pub end: u32,
    /// The occurrence declares the symbol (`SymbolRole::Definition`).
    pub definition: bool,
    /// The occurrence is an import directive (`SymbolRole::Import`).
    pub import: bool,
    /// Last line (0-based) of the construct the occurrence defines, from `enclosing_range`.
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipDocument {
    /// As the indexer wrote it, relative to the index's project root, with `/`.
    pub path: String,
    pub encoding: Encoding,
    pub occurrences: Vec<ScipOccurrence>,
    /// `(implementer, base)`: the first symbol implements or overrides the second.
    pub implementations: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipIndex {
    /// The indexer's own name, for messages.
    pub tool: String,
    /// The `file://` URI documents are relative to; empty when the index has none.
    pub project_root: String,
    pub documents: Vec<ScipDocument>,
}

/// Decodes a `.scip` file. The error is a plain sentence for the user.
pub fn parse_scip(bytes: &[u8]) -> Result<ScipIndex, String> {
    let index = Index::parse_from_bytes(bytes).map_err(|err| format!("not a SCIP index: {err}"))?;
    let metadata = index.metadata.as_ref();
    let documents = index
        .documents
        .iter()
        .filter(|doc| !doc.relative_path.is_empty())
        .map(|doc| {
            let encoding = match doc.position_encoding.enum_value() {
                Ok(PositionEncoding::UTF8CodeUnitOffsetFromLineStart) => Encoding::Utf8,
                Ok(PositionEncoding::UTF32CodeUnitOffsetFromLineStart) => Encoding::Utf32,
                // Unspecified is what the older indexers wrote, and they counted UTF-16 units.
                _ => Encoding::Utf16,
            };
            let occurrences = doc
                .occurrences
                .iter()
                .filter(|occ| is_global(&occ.symbol))
                .filter_map(|occ| {
                    let (line, start, end) = single_line(occ)?;
                    Some(ScipOccurrence {
                        symbol: occ.symbol.clone(),
                        line,
                        start,
                        end,
                        definition: occ.symbol_roles & SymbolRole::Definition as i32 != 0,
                        import: occ.symbol_roles & SymbolRole::Import as i32 != 0,
                        end_line: enclosing_end(occ),
                    })
                })
                .collect();
            let mut implementations = Vec::new();
            for info in doc.symbols.iter().filter(|info| is_global(&info.symbol)) {
                for relation in info.relationships.iter().filter(|r| r.is_implementation) {
                    if is_global(&relation.symbol) {
                        implementations.push((info.symbol.clone(), relation.symbol.clone()));
                    }
                }
            }
            ScipDocument {
                path: doc.relative_path.replace('\\', "/"),
                encoding,
                occurrences,
                implementations,
            }
        })
        .collect();
    Ok(ScipIndex {
        tool: metadata
            .and_then(|m| m.tool_info.as_ref())
            .map(|t| t.name.clone())
            .unwrap_or_default(),
        project_root: metadata.map(|m| m.project_root.clone()).unwrap_or_default(),
        documents,
    })
}

fn is_global(symbol: &str) -> bool {
    !symbol.is_empty() && !symbol.starts_with("local ")
}

/// The last line of the enclosing range. The typed field comes first because scip-go writes only
/// it; the deprecated `enclosing_range` has the same two forms as `range`.
fn enclosing_end(occ: &scip::types::Occurrence) -> Option<u32> {
    let last = match &occ.typed_enclosing_range {
        Some(Typed_enclosing_range::SingleLineEnclosingRange(r)) => r.line,
        Some(Typed_enclosing_range::MultiLineEnclosingRange(r)) => r.end_line,
        _ => match occ.enclosing_range.as_slice() {
            [line, _, _] => *line,
            [_, _, end_line, _] => *end_line,
            _ => return None,
        },
    };
    u32::try_from(last).ok()
}

/// A definition's SCIP symbol read as a declaration of ours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipSymbol {
    /// The descriptor names joined with `.`.
    pub qname: String,
    /// The last descriptor's name.
    pub name: String,
    /// One of our symbol kinds: `module`, `type`, `member`, `method` or `function`.
    pub kind: &'static str,
    /// The `qname` without the last descriptor; `None` for a top-level symbol.
    pub parent: Option<String>,
}

/// Reads a global SCIP symbol string. `None` for a local symbol, a string that does not parse, and
/// a symbol that is not a declaration of ours (a parameter, a type parameter, a macro, metadata).
pub fn scip_symbol(symbol: &str) -> Option<ScipSymbol> {
    use scip::types::descriptor::Suffix;
    if !is_global(symbol) {
        return None;
    }
    let parsed = scip::symbol::parse_symbol(symbol).ok()?;
    let (last, before) = parsed.descriptors.split_last()?;
    let kind = match last.suffix.enum_value().ok()? {
        Suffix::Namespace | Suffix::Package => "module",
        Suffix::Type => "type",
        Suffix::Term => "member",
        Suffix::Method => {
            let in_type = before.iter().any(|d| d.suffix.enum_value() == Ok(Suffix::Type));
            if in_type { "method" } else { "function" }
        }
        _ => return None,
    };
    let names: Vec<&str> = before.iter().map(|d| d.name.as_str()).collect();
    Some(ScipSymbol {
        qname: names
            .iter()
            .copied()
            .chain([last.name.as_str()])
            .collect::<Vec<_>>()
            .join("."),
        name: last.name.clone(),
        kind,
        parent: (!names.is_empty()).then(|| names.join(".")),
    })
}

/// `(line, start, end)` of a range that sits on one line, from either encoding of it.
fn single_line(occ: &scip::types::Occurrence) -> Option<(u32, u32, u32)> {
    let (line, start, end) = match occ.range.as_slice() {
        [line, start, end] => (*line, *start, *end),
        [line, start, end_line, end] if line == end_line => (*line, *start, *end),
        [] => match &occ.typed_range {
            Some(Typed_range::SingleLineRange(r)) => (r.line, r.start_character, r.end_character),
            _ => return None,
        },
        _ => return None,
    };
    let (line, start, end) = (
        u32::try_from(line).ok()?,
        u32::try_from(start).ok()?,
        u32::try_from(end).ok()?,
    );
    (end > start).then_some((line, start, end))
}

/// The text of columns `start..end` of `line` in `encoding`, or `None` when the
/// range does not fall on character boundaries inside the line (a stale index).
pub fn slice_line(line: &str, start: u32, end: u32, encoding: Encoding) -> Option<&str> {
    let (start, end) = (start as usize, end as usize);
    let offset = |column: usize| -> Option<usize> {
        match encoding {
            Encoding::Utf8 => (column <= line.len() && line.is_char_boundary(column)).then_some(column),
            Encoding::Utf16 | Encoding::Utf32 => {
                let mut units = 0;
                for (at, ch) in line.char_indices() {
                    if units == column {
                        return Some(at);
                    }
                    if units > column {
                        return None;
                    }
                    units += if encoding == Encoding::Utf16 { ch.len_utf16() } else { 1 };
                }
                (units == column).then_some(line.len())
            }
        }
    };
    line.get(offset(start)?..offset(end)?)
}
