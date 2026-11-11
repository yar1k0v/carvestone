//! Split and join `---` YAML frontmatter and the Markdown body.
//!
//! Frontmatter is a restricted YAML subset: top-level keys only, each value a
//! scalar, an inline list `[a, b]` or a one-level inline map `{ k: v }`. Every
//! key therefore sits on one line, and edits are line patches on the raw text:
//! `set` and `remove` touch only the lines of the key they change, so an
//! unchanged file is written back byte for byte and
//! a changed one keeps its comments, order, quoting and line endings elsewhere.

use std::ops::Range;

use carvestone_core::item::Stamp;
use serde_norway::{Mapping, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrontmatterError {
    #[error("missing frontmatter: file must start with ---")]
    Missing,
    #[error("unterminated frontmatter: no closing ---")]
    Unterminated,
    #[error("frontmatter is not a mapping")]
    NotAMapping,
    #[error("invalid YAML: {0}")]
    Yaml(#[from] serde_norway::Error),
    #[error("invalid key `{0}`: keys are plain words (letters, digits, `_`, `-`, `.`)")]
    InvalidKey(String),
    #[error("unsupported value for `{key}`: {reason}")]
    UnsupportedValue { key: String, reason: &'static str },
    #[error(
        "cannot patch `{key}` at line {line}: write the value inline on one line, like `key: [a, b]` or `key: {{ k: v }}`"
    )]
    Unpatchable { key: String, line: usize },
    #[error("malformed `{key}`: {reason}")]
    MalformedStamp { key: String, reason: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    raw_frontmatter: String,
    /// Parsed view of `raw_frontmatter`, refreshed after every patch.
    fields: Mapping,
    pub body: String,
    /// Line ending of the opening `---`; every line we write uses it.
    eol: &'static str,
    /// The closing `---` line as found (it has no newline at end of file).
    closing: String,
}

impl Document {
    pub fn parse(text: &str) -> Result<Self, FrontmatterError> {
        let (rest, eol) = match (text.strip_prefix("---\r\n"), text.strip_prefix("---\n")) {
            (Some(rest), _) => (rest, "\r\n"),
            (None, Some(rest)) => (rest, "\n"),
            (None, None) => return Err(FrontmatterError::Missing),
        };
        let end = find_closing(rest).ok_or(FrontmatterError::Unterminated)?;
        let raw = &rest[..end.start];
        Ok(Self {
            raw_frontmatter: raw.to_string(),
            fields: parse_fields(raw)?,
            body: rest[end.end..].to_string(),
            eol,
            closing: rest[end.start..end.end].to_string(),
        })
    }

    pub fn fields(&self) -> &Mapping {
        &self.fields
    }

    /// String field, without YAML's implicit typing: `1.40` stays `"1.40"`.
    pub fn get_str(&self, key: &str) -> Option<String> {
        match self.fields.get(key)? {
            Value::String(s) => Some(s.clone()),
            // Re-read the raw scalar so numbers keep their written form.
            Value::Number(_) | Value::Bool(_) => raw_scalar(&self.raw_frontmatter, key),
            _ => None,
        }
    }

    /// Line ending of the file, so callers that write lines match it.
    pub fn eol(&self) -> &'static str {
        self.eol
    }

    /// File line (1-based, opening `---` is line 1) of a top-level key; continuation lines never match.
    pub fn key_line(&self, key: &str) -> Option<usize> {
        self.raw_frontmatter
            .split_inclusive('\n')
            .position(|l| split_entry(l).is_some_and(|(k, _)| k == key))
            .map(|i| i + 2)
    }

    /// File line of the body's first line. The closing `---` has no newline at end of
    /// file, but the body line still counts.
    pub fn body_line(&self) -> usize {
        self.raw_frontmatter.matches('\n').count() + 3
    }

    /// List field as strings, numbers in their written form; a bare scalar is a one-item list.
    pub fn get_list(&self, key: &str) -> Option<Vec<String>> {
        let raw = raw_value(&self.raw_frontmatter, key);
        match self.fields.get(key)? {
            Value::Null => Some(vec![]),
            Value::Sequence(items) => items
                .iter()
                .zip(child_raws(raw, items.len(), false))
                .map(|(item, r)| scalar_text(item, r))
                .collect(),
            Value::Mapping(_) | Value::Tagged(_) => None,
            v => scalar_text(v, raw).map(|t| vec![t]),
        }
    }

    /// `generated`/`verified` style stamps: a list of `{ by, at }` or one bare map.
    pub fn get_stamps(&self, key: &str) -> Result<Vec<Stamp>, FrontmatterError> {
        let bad = || FrontmatterError::MalformedStamp {
            key: key.into(),
            reason: STAMP_SHAPE,
        };
        let raw = raw_value(&self.raw_frontmatter, key);
        match self.fields.get(key) {
            None | Some(Value::Null) => Ok(vec![]),
            Some(v @ Value::Mapping(_)) => stamp_of(v, raw).map(|s| vec![s]).ok_or_else(bad),
            Some(Value::Sequence(items)) => items
                .iter()
                .zip(child_raws(raw, items.len(), false))
                .map(|(v, r)| stamp_of(v, r).ok_or_else(bad))
                .collect(),
            Some(_) => Err(bad()),
        }
    }

    /// All fields as JSON in file order; numbers keep their written text.
    pub fn json_fields(&self) -> serde_json::Map<String, serde_json::Value> {
        self.fields
            .iter()
            .filter_map(|(k, v)| {
                let key = scalar_text(k, None)?;
                let raw = raw_value(&self.raw_frontmatter, &key);
                Some((key, to_json(v, raw)))
            })
            .collect()
    }

    /// Each frontmatter line outside the patchable subset, as (file line, why).
    pub fn subset_problems(&self) -> Vec<(usize, String)> {
        const NOT_ENTRY: &str = "not a `key: value` line; write one top-level key per line";
        let mut out = Vec::new();
        for (n, line) in self.raw_frontmatter.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let reason = match split_entry(line) {
                Some((key, _)) if key.starts_with(['{', '[']) => Some(NOT_ENTRY),
                Some((_, value)) => entry_problem(strip_comment(value).trim()),
                None if is_continuation(line) && t.starts_with('-') => {
                    Some("block list item; write the list inline, like `[a, b]`")
                }
                None if is_continuation(line) => {
                    Some("indented line (block map or multi-line value); write the value inline, like `{ k: v }`")
                }
                None => Some(NOT_ENTRY),
            };
            if let Some(reason) = reason {
                out.push((n + 2, reason.to_string()));
            }
        }
        out
    }

    /// Replace the line(s) of a top-level key, or append `key: value` at the
    /// end of the frontmatter. Setting the value a key already has changes nothing.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), FrontmatterError> {
        check_key(key)?;
        let text = emit_value(&value).map_err(|reason| FrontmatterError::UnsupportedValue {
            key: key.into(),
            reason,
        })?;
        if self.fields.get(key) == Some(&value) {
            return Ok(());
        }
        let line = format!("{key}: {text}{}", self.eol);
        let mut raw = self.raw_frontmatter.clone();
        match find_entry(&raw, key) {
            Some(range) => raw.replace_range(range, &line),
            None if self.fields.contains_key(key) => return Err(self.unpatchable(key)),
            None => raw.push_str(&line),
        }
        self.commit(raw, key, true)
    }

    /// Delete a top-level key's line(s). Returns whether the key was there.
    pub fn remove(&mut self, key: &str) -> Result<bool, FrontmatterError> {
        let Some(range) = find_entry(&self.raw_frontmatter, key) else {
            return if self.fields.contains_key(key) {
                Err(self.unpatchable(key))
            } else {
                Ok(false)
            };
        };
        let mut raw = self.raw_frontmatter.clone();
        raw.replace_range(range, "");
        self.commit(raw, key, false)?;
        Ok(true)
    }

    pub fn to_text(&self) -> String {
        format!("---{}{}{}{}", self.eol, self.raw_frontmatter, self.closing, self.body)
    }

    /// Adopt patched text only if it still parses and the key ended up where we meant.
    fn commit(&mut self, raw: String, key: &str, present: bool) -> Result<(), FrontmatterError> {
        let fields = parse_fields(&raw)?;
        if fields.contains_key(key) != present {
            return Err(self.unpatchable(key));
        }
        self.raw_frontmatter = raw;
        self.fields = fields;
        Ok(())
    }

    /// The error for a key we cannot patch. The line is the key's own line, else the
    /// first line outside the subset, else the first frontmatter line.
    fn unpatchable(&self, key: &str) -> FrontmatterError {
        let line = self
            .key_line(key)
            .or_else(|| self.subset_problems().first().map(|p| p.0))
            .unwrap_or(2);
        FrontmatterError::Unpatchable { key: key.into(), line }
    }
}

fn parse_fields(raw: &str) -> Result<Mapping, FrontmatterError> {
    match serde_norway::from_str::<Value>(raw)? {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        _ => Err(FrontmatterError::NotAMapping),
    }
}

struct Span {
    start: usize,
    end: usize,
}

/// The first line that is exactly `---`, as byte offsets into `rest`.
fn find_closing(rest: &str) -> Option<Span> {
    let mut start = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some(Span {
                start,
                end: start + line.len(),
            });
        }
        start += line.len();
    }
    None
}

/// Key and the text after its colon, when the line starts a top-level entry
/// (no leading whitespace, not a comment or list item). An inline `at:` inside
/// `generated: { .. }` is not an entry of its own.
fn split_entry(line: &str) -> Option<(&str, &str)> {
    let l = line.trim_end_matches(['\r', '\n']);
    let first = l.chars().next()?;
    if first.is_whitespace() || first == '#' || l == "-" || l.starts_with("- ") {
        return None;
    }
    let (key, rest) = if matches!(first, '\'' | '"') {
        let inner = &l[1..];
        let close = inner.find(first)?;
        (&inner[..close], &inner[close + 1..])
    } else {
        // The key ends at the first `:` followed by a blank or the end of the line.
        let colon = l
            .char_indices()
            .find(|&(i, c)| c == ':' && matches!(l[i + 1..].chars().next(), None | Some(' ' | '\t')))?
            .0;
        (l[..colon].trim_end(), &l[colon..])
    };
    let value = rest.trim_start().strip_prefix(':')?;
    (value.is_empty() || value.starts_with([' ', '\t'])).then_some((key, value.trim_start()))
}

/// Continuation of a multi-line entry written by hand (block list or map).
fn is_continuation(line: &str) -> bool {
    let l = line.trim_end_matches(['\r', '\n']);
    l.starts_with([' ', '\t']) || l == "-" || l.starts_with("- ")
}

/// Byte range of a top-level key's entry: its line plus any continuation lines.
fn find_entry(raw: &str, key: &str) -> Option<Range<usize>> {
    let mut entry: Option<Range<usize>> = None;
    let mut pos = 0;
    for line in raw.split_inclusive('\n') {
        let next = pos + line.len();
        if let Some(range) = &mut entry {
            if !is_continuation(line) {
                break;
            }
            range.end = next;
        } else if split_entry(line).is_some_and(|(k, _)| k == key) {
            entry = Some(pos..next);
        }
        pos = next;
    }
    entry
}

fn raw_scalar(raw: &str, key: &str) -> Option<String> {
    raw_value(raw, key).map(str::to_string)
}

/// Text after the key's colon on a single-line entry, comment removed. `None` for an
/// absent key or an empty value (null, or a block that continues on the next lines).
fn raw_value<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    raw.lines().find_map(|l| {
        let (k, v) = split_entry(l)?;
        if k != key {
            return None;
        }
        let v = strip_comment(v).trim();
        (!v.is_empty()).then_some(v)
    })
}

/// Byte offset and char of every character outside quotes. A quote opens only at the
/// start of a token (after a space, `[`, `{`, `,` or `:`), so `it's` stays plain text.
fn unquoted(text: &str) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut prev: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match quote {
            Some('"') if c == '\\' => {
                chars.next();
            }
            Some('\'') if c == '\'' && chars.peek().is_some_and(|&(_, n)| n == '\'') => {
                chars.next();
            }
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if matches!(c, '\'' | '"') && prev.is_none_or(|p| p.is_whitespace() || "[{,:".contains(p)) => {
                quote = Some(c);
            }
            None => out.push((i, c)),
        }
        prev = Some(c);
    }
    out
}

/// Cuts a trailing ` # comment` that sits outside quotes.
fn strip_comment(v: &str) -> &str {
    for (i, c) in unquoted(v) {
        if c == '#' && v[..i].chars().next_back().is_none_or(char::is_whitespace) {
            return &v[..i];
        }
    }
    v
}

/// Splits the inside of `[..]` or `{..}` at the commas that are outside quotes and nested brackets.
fn split_top(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in unquoted(inner) {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Items of an inline `[..]` list or `{..}` map as written; empty for anything else.
fn flow_children(text: &str) -> Vec<&str> {
    let t = text.trim();
    let list = t.strip_prefix('[').and_then(|r| r.strip_suffix(']'));
    let map = t.strip_prefix('{').and_then(|r| r.strip_suffix('}'));
    list.or(map).map(split_top).unwrap_or_default()
}

/// The value text of a `key: value` map entry as written.
fn pair_value(pair: &str) -> &str {
    unquoted(pair)
        .into_iter()
        .find(|&(i, c)| c == ':' && pair[i + 1..].chars().next().is_none_or(char::is_whitespace))
        .map_or("", |(i, _)| pair[i + 1..].trim())
}

/// Written text of each child of an inline list (`pairs` false) or map (`pairs` true).
/// All `None` when the value is not inline or its parsed length disagrees, which only
/// loses the written text of numbers, never a value.
fn child_raws(raw: Option<&str>, len: usize, pairs: bool) -> Vec<Option<&str>> {
    let children = raw.map(flow_children).unwrap_or_default();
    if children.len() != len {
        return vec![None; len];
    }
    children
        .into_iter()
        .map(|c| Some(if pairs { pair_value(c) } else { c }))
        .collect()
}

/// A scalar as text. Numbers keep the text they were written with when `raw` has it.
fn scalar_text(value: &Value, raw: Option<&str>) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(raw.map_or_else(|| n.to_string(), str::to_string)),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some("null".into()),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => None,
    }
}

const STAMP_SHAPE: &str = "expected `{ by: .., at: .. }` or a list of them";

/// `{ by, at }` from a parsed map; other keys are ignored, a missing or null `by`/`at` is `None`.
fn stamp_of(value: &Value, raw: Option<&str>) -> Option<Stamp> {
    let Value::Mapping(m) = value else {
        return None;
    };
    let raws = child_raws(raw, m.len(), true);
    let (mut by, mut at) = (None, None);
    for ((k, v), r) in m.iter().zip(raws) {
        if v.is_null() {
            continue;
        }
        match k.as_str() {
            Some("by") => by = scalar_text(v, r),
            Some("at") => at = scalar_text(v, r),
            _ => {}
        }
    }
    Some(Stamp { by: by?, at: at? })
}

/// JSON of a parsed value: numbers become strings with their written text, bools and null stay typed.
fn to_json(value: &Value, raw: Option<&str>) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        Value::Null | Value::Tagged(_) => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::String(s) => J::String(s.clone()),
        Value::Number(n) => J::String(raw.map_or_else(|| n.to_string(), str::to_string)),
        Value::Sequence(items) => {
            let raws = child_raws(raw, items.len(), false);
            J::Array(items.iter().zip(raws).map(|(v, r)| to_json(v, r)).collect())
        }
        Value::Mapping(m) => {
            let raws = child_raws(raw, m.len(), true);
            J::Object(
                m.iter()
                    .zip(raws)
                    .filter_map(|((k, v), r)| Some((scalar_text(k, None)?, to_json(v, r))))
                    .collect(),
            )
        }
    }
}

/// Why a single-line entry's value is outside the subset, if it is.
fn entry_problem(value: &str) -> Option<&'static str> {
    if value.starts_with(['&', '*', '!']) {
        Some("anchors, aliases and tags are not supported")
    } else if value.starts_with(['|', '>']) {
        Some("block scalar; write the value inline on one line")
    } else if (value.starts_with('[') && !value.ends_with(']')) || (value.starts_with('{') && !value.ends_with('}')) {
        Some("inline list or map continues on the next line; write it on one line")
    } else {
        None
    }
}

fn is_simple_key(key: &str) -> bool {
    key.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')
        && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !needs_quote(key, true)
}

fn check_key(key: &str) -> Result<(), FrontmatterError> {
    if is_simple_key(key) {
        Ok(())
    } else {
        Err(FrontmatterError::InvalidKey(key.into()))
    }
}

/// Canonical text of a value in the restricted subset, or why it is outside it.
fn emit_value(value: &Value) -> Result<String, &'static str> {
    match value {
        Value::Sequence(items) => {
            let parts = items
                .iter()
                .map(|i| match i {
                    Value::Mapping(m) => emit_map(m),
                    other => emit_scalar(other, true),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", parts.join(", ")))
        }
        Value::Mapping(m) => emit_map(m),
        other => emit_scalar(other, false),
    }
}

fn emit_map(m: &Mapping) -> Result<String, &'static str> {
    if m.is_empty() {
        return Ok("{}".into());
    }
    let parts = m
        .iter()
        .map(|(k, v)| {
            let Value::String(k) = k else {
                return Err("map keys must be strings");
            };
            if !is_simple_key(k) {
                return Err("map keys must be plain words");
            }
            Ok(format!("{k}: {}", emit_scalar(v, true)?))
        })
        .collect::<Result<Vec<String>, &'static str>>()?;
    Ok(format!("{{ {} }}", parts.join(", ")))
}

/// `flow` is true inside `[..]` and `{..}`, where more characters are special.
fn emit_scalar(value: &Value, flow: bool) -> Result<String, &'static str> {
    match value {
        Value::Null => Ok("null".into()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => emit_string(s, flow),
        Value::Sequence(_) | Value::Mapping(_) => Err("nested lists and maps are not supported"),
        Value::Tagged(_) => Err("tagged values are not supported"),
    }
}

fn emit_string(s: &str, flow: bool) -> Result<String, &'static str> {
    if s.chars().any(|c| c.is_control() && c != '\t') {
        return Err("multi-line values and control characters are not supported");
    }
    Ok(if needs_quote(s, flow) {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    })
}

/// Characters that end or confuse a plain scalar inside `[..]` / `{..}`. The
/// colon is included because libyaml-style parsers differ on `a:b` in flow context.
const FLOW_UNSAFE: [char; 7] = [',', '[', ']', '{', '}', ':', '?'];

/// Whether a string must be single-quoted to read back as the same string.
/// ISO dates stay plain: the parser has no timestamp type, so they read as
/// text, and data files already write them that way (`created: 2026-10-16`).
fn needs_quote(s: &str, flow: bool) -> bool {
    let first = s.chars().next();
    s.is_empty()
        || s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.contains('\t')
        || first.is_some_and(|c| ",[]{}#&*!|>'\"%@`".contains(c))
        // `- x`, `? x`, `: x` are block indicators; `-x` is a plain string.
        || (matches!(first, Some('-' | '?' | ':')) && s[1..].chars().next().is_none_or(char::is_whitespace))
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || (flow && s.contains(FLOW_UNSAFE))
        || is_reserved_word(s)
        || looks_like_number(s)
}

/// YAML 1.2 null/bool plus the 1.1 spellings other tools still read as such.
fn is_reserved_word(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "null" | "~" | "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n"
    )
}

/// Anything a YAML parser may type as a number: `1.40`, `007`, `1e3`, `0x1F`, `1_000`, `.inf`.
fn looks_like_number(s: &str) -> bool {
    let t = s.strip_prefix(['+', '-']).unwrap_or(s).to_ascii_lowercase();
    let radix = |prefix: &str, base: u32| {
        t.strip_prefix(prefix)
            .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_digit(base)))
    };
    radix("0x", 16)
        || radix("0o", 8)
        || radix("0b", 2)
        || matches!(t.as_str(), ".inf" | ".nan")
        || t.replace('_', "").parse::<f64>().is_ok()
}
