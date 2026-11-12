//! Markdown sections of an item body: GitHub-style anchors, line ranges and
//! minimal-diff edits. Line numbers are file lines, so callers pass the file
//! line of the body's first line.

use std::collections::HashMap;

use serde::Serialize;

use crate::error::Error;

/// A heading and everything under it up to the next heading of the same or a
/// higher level. Lines are file lines, 1-based, inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Section {
    pub anchor: String,
    pub title: String,
    pub level: u8,
    pub start_line: usize,
    pub end_line: usize,
}

struct Line<'a> {
    start: usize,
    text: &'a str,
    in_fence: bool,
    /// The opening or the closing line of a fence.
    opens: bool,
    closes: bool,
    heading: Option<(u8, String)>,
}

/// Length of the leading run of `c` in `s`.
fn run_of(s: &str, c: char) -> usize {
    s.chars().take_while(|&x| x == c).count()
}

fn heading_of(text: &str) -> Option<(u8, String)> {
    let indent = run_of(text, ' ');
    if indent > 3 {
        return None;
    }
    let rest = &text[indent..];
    let level = run_of(rest, '#');
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &rest[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let mut title = rest.trim();
    let stripped = title.trim_end_matches('#');
    if stripped.len() != title.len() && (stripped.is_empty() || stripped.ends_with(' ')) {
        title = stripped.trim_end();
    }
    Some((level as u8, title.to_string()))
}

/// The single place that decides what is a fence and what is a heading.
fn scan(body: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut start = 0;
    for raw in body.split_inclusive('\n') {
        let text = raw.strip_suffix('\n').unwrap_or(raw);
        let text = text.strip_suffix('\r').unwrap_or(text);
        let trimmed = text.trim_start();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let mut in_fence = fence.is_some();
        let (mut opens, mut closes) = (false, false);
        if let Some(c) = marker {
            let n = run_of(trimmed, c);
            match fence {
                None if n >= 3 => {
                    fence = Some((c, n));
                    in_fence = true;
                    opens = true;
                }
                Some((fc, fn_)) if fc == c && n >= fn_ && trimmed[n..].trim().is_empty() => {
                    fence = None;
                    closes = true;
                }
                _ => {}
            }
        }
        let heading = if in_fence { None } else { heading_of(text) };
        lines.push(Line {
            start,
            text,
            in_fence,
            opens,
            closes,
            heading,
        });
        start += raw.len();
    }
    lines
}

/// GitHub-style anchor for a heading.
pub fn slug(heading: &str) -> String {
    heading
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || matches!(c, '-' | '_') => Some(c),
            _ => None,
        })
        .collect()
}

/// `first_line` is the file line of the body's first line.
pub fn sections(body: &str, first_line: usize) -> Vec<Section> {
    let lines = scan(body);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((level, title)) = &line.heading else {
            continue;
        };
        let base = slug(title);
        let count = seen.entry(base.clone()).or_insert(0);
        let anchor = if *count == 0 { base } else { format!("{base}-{count}") };
        *count += 1;
        let next = lines[i + 1..]
            .iter()
            .position(|l| l.heading.as_ref().is_some_and(|(lv, _)| lv <= level))
            .map_or(lines.len(), |p| i + 1 + p);
        out.push(Section {
            anchor,
            title: title.clone(),
            level: *level,
            start_line: first_line + i,
            end_line: first_line + next - 1,
        });
    }
    out
}

/// The section's text with its trailing blank lines and final newline. A
/// section that is not from this body gives `""`.
pub fn section_text<'a>(body: &'a str, first_line: usize, section: &Section) -> &'a str {
    let lines = scan(body);
    let (Some(lo), Some(hi)) = (
        section.start_line.checked_sub(first_line),
        section.end_line.checked_sub(first_line),
    ) else {
        return "";
    };
    let Some(first) = lines.get(lo) else {
        return "";
    };
    let end = lines.get(hi + 1).map_or(body.len(), |l| l.start);
    body.get(first.start..end).unwrap_or("")
}

fn find<'a>(secs: &'a [Section], name: &str) -> Option<&'a Section> {
    let name = name.trim();
    secs.iter()
        .find(|s| s.title.eq_ignore_ascii_case(name))
        .or_else(|| secs.iter().find(|s| s.anchor == name.to_lowercase()))
}

/// The line ending of the first line: `\r\n` or `\n`.
pub fn eol(body: &str) -> &'static str {
    match body.find('\n') {
        Some(i) if body[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// Replaces the named section's text, or appends the section when missing.
/// Everything outside that section stays byte-identical.
pub fn set_section(body: &str, name: &str, text: &str) -> String {
    let eol = eol(body);
    let t = text.trim().replace("\r\n", "\n").replace('\n', eol);
    let secs = sections(body, 1);
    let Some(s) = find(&secs, name) else {
        let mut out = body.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        let before = out.strip_suffix('\n').unwrap_or(&out);
        if !(out.is_empty() || before.ends_with('\n') || before.ends_with("\n\r")) {
            out.push_str(eol);
        }
        let name = name.trim();
        // An anchor like `acceptance-criteria` reads as a title with spaces.
        let is_anchor = name.contains('-') && !name.contains(' ') && name.chars().all(|c| !c.is_uppercase());
        let title = if is_anchor {
            name.replace('-', " ")
        } else {
            name.to_string()
        };
        let mut chars = title.chars();
        out.push_str("## ");
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
        out.push_str(eol);
        out.push_str(eol);
        out.push_str(&t);
        out.push_str(eol);
        return out;
    };
    let lines = scan(body);
    let heading_end = lines.get(s.start_line).map_or(body.len(), |l| l.start);
    let region_end = lines.get(s.end_line).map_or(body.len(), |l| l.start);
    let mut out = body[..heading_end].to_string();
    if !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(eol);
    out.push_str(&t);
    out.push_str(eol);
    if region_end < body.len() {
        out.push_str(eol);
    }
    out.push_str(&body[region_end..]);
    out
}

/// Removes the named section: its heading and everything under it, sub-sections included.
/// Every other byte stays. `None` when the body has no such section.
pub fn remove_section(body: &str, name: &str) -> Option<String> {
    let secs = sections(body, 1);
    let s = find(&secs, name)?;
    let lines = scan(body);
    let start = lines[s.start_line - 1].start;
    let end = lines.get(s.end_line).map_or(body.len(), |l| l.start);
    if end < body.len() {
        return Some(format!("{}{}", &body[..start], &body[end..]));
    }
    // The last section: the blank line that parted it from the one above goes too.
    let kept = body[..start].trim_end_matches(['\r', '\n']);
    Some(if kept.is_empty() {
        String::new()
    } else {
        format!("{kept}{}", eol(body))
    })
}

/// One `- [ ]` / `- [x]` line of a section, numbered from 1 in the order
/// `check_box` counts them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Checkbox {
    pub n: usize,
    pub text: String,
    pub checked: bool,
}

/// The checkboxes of a section (its subsections included) with the byte
/// offset of each box's mark in `body`. Fenced lines do not count.
fn boxes_in(body: &str, first_line: usize, section: &Section) -> Vec<(usize, Checkbox)> {
    let lines = scan(body);
    let (Some(lo), Some(hi)) = (
        section.start_line.checked_sub(first_line),
        section.end_line.checked_sub(first_line),
    ) else {
        return Vec::new();
    };
    let Some(region) = lines.get(lo..=hi) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in region.iter().filter(|l| !l.in_fence) {
        let trimmed = line.text.trim_start();
        let Some(rest) = trimmed.strip_prefix("- [") else {
            continue;
        };
        let (mark, rest) = (rest.chars().next(), rest.get(1..));
        let (Some(mark @ (' ' | 'x' | 'X')), Some(text)) = (mark, rest.and_then(|r| r.strip_prefix(']'))) else {
            continue;
        };
        out.push((
            line.start + (line.text.len() - trimmed.len()) + 3,
            Checkbox {
                n: out.len() + 1,
                text: text.trim().to_string(),
                checked: mark != ' ',
            },
        ));
    }
    out
}

/// The checkboxes of a section, in the numbering `check_box` uses.
pub fn checkboxes(body: &str, first_line: usize, section: &Section) -> Vec<Checkbox> {
    boxes_in(body, first_line, section)
        .into_iter()
        .map(|(_, b)| b)
        .collect()
}

/// Ticks the nth (1-based) checkbox of a section. Ticked boxes count, so
/// ticking one twice changes nothing.
pub fn check_box(body: &str, section: &str, n: usize) -> Result<String, Error> {
    let secs = sections(body, 1);
    let s = find(&secs, section).ok_or_else(|| Error::SectionNotFound(section.to_string()))?;
    let boxes = boxes_in(body, 1, s);
    if n == 0 || n > boxes.len() {
        return Err(Error::CheckboxOutOfRange {
            section: section.to_string(),
            n,
            count: boxes.len(),
        });
    }
    let pos = boxes[n - 1].0;
    let mut out = body.to_string();
    if body.as_bytes()[pos] == b' ' {
        out.replace_range(pos..=pos, "x");
    }
    Ok(out)
}

/// Body lines with their file lines and whether a fence covers them. The one
/// way a caller reads a body line by line without its own fence rule.
pub fn lines(body: &str, first_line: usize) -> Vec<(usize, &str, bool)> {
    scan(body)
        .iter()
        .enumerate()
        .map(|(i, line)| (first_line + i, line.text, line.in_fence))
        .collect()
}

/// Byte ranges of the inline code spans in one line, delimiters included.
pub fn code_span_ranges(line: &str) -> Vec<(usize, usize)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'`' {
            i += line[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let mut j = i;
        while j < b.len() && b[j] == b'`' {
            j += 1;
        }
        let k = j - i;
        let mut m = j;
        let mut close = None;
        while m < b.len() {
            if b[m] == b'`' {
                let mut e = m;
                while e < b.len() && b[e] == b'`' {
                    e += 1;
                }
                if e - m == k {
                    close = Some(e);
                    break;
                }
                m = e;
            } else {
                m += 1;
            }
        }
        match close {
            Some(e) => {
                out.push((i, e));
                i = e;
            }
            None => i = j,
        }
    }
    out
}

/// The line with inline code spans replaced by spaces of the same length, so
/// offsets into the result are offsets into the line.
fn without_code_spans(line: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    for (start, end) in code_span_ranges(line) {
        out.push_str(&line[i..start]);
        out.push_str(&" ".repeat(end - start));
        i = end;
    }
    out.push_str(&line[i..]);
    out
}

/// Prose lines with their file line and byte offset in the body: fenced blocks
/// dropped and inline code spans blanked, so every link scan skips code the
/// same way.
fn prose_lines(body: &str, first_line: usize) -> Vec<(usize, usize, String)> {
    scan(body)
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.in_fence)
        .map(|(i, line)| (first_line + i, line.start, without_code_spans(line.text)))
        .collect()
}

/// Markdown of one line as reading text: links become their words, code spans their code, and
/// bold marks go. `[[docs/a-b#c]]` reads `a-b`.
pub fn plain_text(line: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in code_span_ranges(line) {
        out.push_str(&plain_prose(&line[at..start]));
        out.push_str(line[start..end].trim_matches('`').trim());
        at = end;
    }
    out.push_str(&plain_prose(&line[at..]));
    out
}

fn plain_prose(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        if let Some(inner) = after.strip_prefix("[[")
            && let Some(close) = inner.find("]]")
        {
            let target = &inner[..close];
            let shown = target.rsplit_once('|').map_or(target, |(_, label)| label);
            let shown = shown.split('#').next().unwrap_or(shown);
            out.push_str(shown.rsplit('/').next().unwrap_or(shown).trim());
            rest = &inner[close + 2..];
        } else if let Some(close) = after.find("](")
            && let Some(end) = after[close..].find(')')
        {
            out.push_str(&after[1..close]);
            rest = &after[close + end + 1..];
        } else {
            out.push('[');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "")
}

/// Labels of a line that records a fact about the doc rather than saying what it is about.
const FACT_LABELS: [&str; 12] = [
    "status",
    "type",
    "date",
    "jira",
    "plan",
    "related",
    "discovered",
    "sources used",
    "method note",
    "pages",
    "last-verified",
    "last verified",
];
/// Labels that only introduce the sentence after them.
const LEAD_LABELS: [&str; 4] = ["purpose", "problem", "summary", "goal"];

/// The label of a `Label: text` line and the text, when the label is one of `labels`.
fn labelled<'a>(line: &'a str, labels: &[&str]) -> Option<&'a str> {
    let (label, rest) = line.split_once(':')?;
    let label = label.trim_matches(['*', '_']);
    labels
        .iter()
        .any(|known| label.eq_ignore_ascii_case(known))
        .then_some(rest.trim_start_matches(['*', '_']).trim())
}

/// `text` with its first letter a capital, as a sentence that lost its label starts.
fn sentence(text: String) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

/// True for a line of running prose: not a table, list, quote, tag or a line of facts.
fn is_prose(line: &str) -> bool {
    !line.contains(" · ")
        && labelled(line, &FACT_LABELS).is_none()
        && !["|", "- ", "* ", "+ ", ">", "<", "![", "---"]
            .iter()
            .any(|mark| line.starts_with(mark))
}

/// The text of a list item's first line.
fn bullet<'a>(line: &Line<'a>) -> Option<&'a str> {
    let text = line.text.trim();
    if line.in_fence {
        return None;
    }
    ["- ", "* ", "+ "].iter().find_map(|mark| text.strip_prefix(mark))
}

/// The first list item of `lines`, wrapped lines included, without its marker.
fn first_list_item(lines: &[Line<'_>]) -> Option<String> {
    let start = lines.iter().position(|line| bullet(line).is_some())?;
    let wrapped = lines[start + 1..]
        .iter()
        .take_while(|line| !line.text.trim().is_empty() && line.heading.is_none() && bullet(line).is_none())
        .map(|line| line.text.trim());
    let item: Vec<&str> = bullet(&lines[start]).into_iter().chain(wrapped).collect();
    Some(plain_text(&item.join(" ")))
}

/// The prose of one paragraph. A line of facts takes the lines wrapped after it along, so what
/// remains starts at a sentence; prose ends at the first line that is not.
fn paragraph_prose(paragraph: &[Line<'_>]) -> Vec<String> {
    let mut words = Vec::new();
    let mut in_facts = false;
    for line in paragraph {
        let text = line.text.trim();
        if let Some(lead) = labelled(text, &LEAD_LABELS) {
            if !words.is_empty() {
                break;
            }
            in_facts = false;
            words.push(sentence(plain_text(lead)));
        } else if !is_prose(text) {
            if !words.is_empty() {
                break;
            }
            in_facts = true;
        } else if !in_facts {
            words.push(plain_text(text));
        }
    }
    words
}

/// The first paragraph of prose in `body`, as reading text on one line. Headings, fences, tables,
/// lists and facts lines are passed over; a body of lists alone opens with its first item.
pub fn first_paragraph(body: &str) -> Option<String> {
    let lines = scan(body);
    lines
        .split(|line| line.in_fence || line.heading.is_some() || line.text.trim().is_empty())
        .map(paragraph_prose)
        .find(|words| !words.is_empty())
        .map(|words| words.join(" "))
        .or_else(|| first_list_item(&lines))
}

/// One `Label: value` fact from the lines that open a doc, such as `Status: live`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// The label in lower case.
    pub label: String,
    /// The value as reading text; several values stay joined with ` · `.
    pub value: String,
}

/// A segment that opens with a short `Label:`; a URL or a sentence with a colon in it does not.
fn fact_of(segment: &str) -> Option<Fact> {
    let (label, rest) = segment.split_once(':')?;
    let label = label.trim().trim_matches(['*', '_']).trim();
    let rest = rest.trim_start_matches(['*', '_']);
    let is_label = (1..=3).contains(&label.split_whitespace().count())
        && label.chars().all(|c| c.is_alphabetic() || c == ' ' || c == '-');
    (is_label && (rest.is_empty() || rest.starts_with(char::is_whitespace))).then(|| Fact {
        label: label.to_lowercase(),
        value: rest.trim().to_string(),
    })
}

/// The facts above a doc's first section, in order. A paragraph holds facts when it opens with a
/// label; lines that wrap are joined, ` · ` separates facts, and a part with no label of its own
/// belongs to the fact before it: `Status: live · Platforms: BE · Web` is two facts.
pub fn doc_facts(body: &str) -> Vec<Fact> {
    let lines = scan(body);
    let lead = lines
        .iter()
        .position(|line| line.heading.as_ref().is_some_and(|(level, _)| *level >= 2))
        .unwrap_or(lines.len());
    let mut facts: Vec<Fact> = Vec::new();
    for paragraph in lines[..lead].split(|line| line.in_fence || line.heading.is_some() || line.text.trim().is_empty())
    {
        let joined = paragraph.iter().map(|l| l.text.trim()).collect::<Vec<_>>().join(" ");
        let mut parts = joined.split('·').map(str::trim).filter(|part| !part.is_empty());
        let Some(first) = parts.next().and_then(fact_of) else {
            continue;
        };
        facts.push(first);
        for part in parts {
            match (fact_of(part), facts.last_mut()) {
                (Some(fact), _) => facts.push(fact),
                (None, Some(last)) => {
                    last.value.push_str(" · ");
                    last.value.push_str(part);
                }
                (None, None) => {}
            }
        }
    }
    for fact in &mut facts {
        fact.value = plain_text(&fact.value).trim().to_string();
    }
    facts
}

/// One `[text](destination)` link of a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// File line of the link.
    pub line: usize,
    /// Byte range of the destination in the body, without `<` `>` or a title.
    pub start: usize,
    pub end: usize,
    /// The destination with its `#anchor`.
    pub target: String,
}

impl Link {
    /// A relative link to a markdown file (not a URL), which is how one stone
    /// item links to another.
    pub fn is_relative_md(&self) -> bool {
        let path = self.target.split_once('#').map_or(self.target.as_str(), |(p, _)| p);
        !self.target.contains(':') && path.ends_with(".md")
    }
}

/// Every `[text](destination)` link of the body in order, images included;
/// fenced blocks and code spans are skipped.
pub fn links(body: &str, first_line: usize) -> Vec<Link> {
    let mut out = Vec::new();
    for (line, line_start, visible) in prose_lines(body, first_line) {
        let mut from = 0;
        while let Some(open) = visible[from..].find("](") {
            let inner_start = from + open + 2;
            let Some(close) = visible[inner_start..].find(')') else {
                break;
            };
            let inner = &visible[inner_start..inner_start + close];
            let lead = inner.len() - inner.trim_start().len();
            let word = inner.split_whitespace().next().unwrap_or("");
            let (mut start, mut end) = (inner_start + lead, inner_start + lead + word.len());
            if visible[start..end].starts_with('<') {
                start += 1;
            }
            if end > start && visible[start..end].ends_with('>') {
                end -= 1;
            }
            if start < end {
                out.push(Link {
                    line,
                    start: line_start + start,
                    end: line_start + end,
                    target: visible[start..end].to_string(),
                });
            }
            from = inner_start + close + 1;
        }
    }
    out
}

/// Targets of relative `[text](x.md)` links with their file lines; URLs,
/// fenced blocks and code spans are skipped. The `#anchor` is kept.
pub fn md_links(body: &str, first_line: usize) -> Vec<(usize, String)> {
    links(body, first_line)
        .into_iter()
        .filter(Link::is_relative_md)
        .map(|link| (link.line, link.target))
        .collect()
}

/// Every `[[target]]` in the body in order, skipping fenced blocks and inline code. `start..end`
/// covers the whole `[[…]]` and the target keeps any `#anchor`.
pub fn wiki_links(body: &str, first_line: usize) -> Vec<Link> {
    let mut out = Vec::new();
    for (line, line_start, visible) in prose_lines(body, first_line) {
        let mut at = 0;
        while let Some(open) = visible[at..].find("[[") {
            let open = at + open;
            let Some(close) = visible[open + 2..].find("]]") else {
                break;
            };
            let end = open + 2 + close + 2;
            let target = visible[open + 2..end - 2].trim();
            if !target.is_empty() {
                out.push(Link {
                    line,
                    start: line_start + open,
                    end: line_start + end,
                    target: target.to_string(),
                });
            }
            at = end;
        }
    }
    out
}

/// Every `[[target]]` in the body with its file line, skipping fenced blocks
/// and inline code. The target keeps any `#anchor`.
pub fn body_links(body: &str, first_line: usize) -> Vec<(usize, String)> {
    wiki_links(body, first_line)
        .into_iter()
        .map(|link| (link.line, link.target))
        .collect()
}

/// One C4 diagram of a body: a ` ```mermaid ` fence whose first line starts with `C4`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagram {
    /// File line of the opening fence.
    pub line: usize,
    /// The diagram's `title` line, when it has one.
    pub title: Option<String>,
    /// The text inside the fence.
    pub source: String,
    /// Every `$link="target"` of a box, with its file line.
    pub links: Vec<(usize, String)>,
}

/// The C4 diagrams of the body in order. Other fences, Mermaid ones included, are skipped.
pub fn diagrams(body: &str, first_line: usize) -> Vec<Diagram> {
    let scanned = scan(body);
    let mut out = Vec::new();
    for (i, open) in scanned.iter().enumerate().filter(|(_, l)| l.opens) {
        let info = open.text.trim_start().trim_start_matches(['`', '~']).trim();
        if info.split_whitespace().next() != Some("mermaid") {
            continue;
        }
        let inner: Vec<(usize, &str)> = scanned[i + 1..]
            .iter()
            .enumerate()
            .take_while(|(_, l)| l.in_fence && !l.closes && !l.opens)
            .map(|(k, l)| (first_line + i + 1 + k, l.text))
            .collect();
        let first = inner.iter().map(|(_, t)| t.trim()).find(|t| !t.is_empty());
        if !first.is_some_and(|t| t.starts_with("C4")) {
            continue;
        }
        let title = inner
            .iter()
            .find_map(|(_, t)| t.trim().strip_prefix("title "))
            .map(|t| t.trim().to_string());
        let links = inner
            .iter()
            .flat_map(|&(line, text)| dollar_links(text).into_iter().map(move |t| (line, t)))
            .collect();
        let source = inner.iter().map(|(_, t)| *t).collect::<Vec<_>>().join("\n");
        out.push(Diagram {
            line: first_line + i,
            title,
            source,
            links,
        });
    }
    out
}

/// One box of a C4 diagram: `Container(api, "API", "Rust", "Serves orders", $link="arch/api")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C4Box {
    /// The macro: `Container`, `ContainerDb`, `System_Ext`, `Person`, ...
    pub kind: String,
    pub alias: String,
    pub label: String,
    /// The quoted arguments after the label (technology and description, or description), in order.
    pub texts: Vec<String>,
    /// The `$link` value, when it has one.
    pub link: Option<String>,
    /// File line.
    pub line: usize,
}

/// One relation of a C4 diagram: `Rel(api, db, "Stores orders", "SQL")`. A `BiRel` is two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C4Rel {
    pub from: String,
    pub to: String,
    pub label: String,
    pub technology: Option<String>,
    /// File line.
    pub line: usize,
    /// The line as written, trimmed.
    pub text: String,
}

/// The boxes and relations of a C4 diagram, in line order. Boundaries, styles and other
/// statements are skipped; their boxes are read like any other line.
pub fn c4_parts(diagram: &Diagram) -> (Vec<C4Box>, Vec<C4Rel>) {
    let (mut boxes, mut rels) = (Vec::new(), Vec::new());
    for (offset, raw) in diagram.source.lines().enumerate() {
        let text = raw.trim();
        let line = diagram.line + 1 + offset;
        let Some((name, rest)) = text.split_once('(') else {
            continue;
        };
        let name = name.trim();
        let Some(inner) = rest.trim_end().trim_end_matches('{').trim_end().strip_suffix(')') else {
            continue;
        };
        let (named, plain): (Vec<String>, Vec<String>) = c4_args(inner).into_iter().partition(|a| a.starts_with('$'));
        let link = named.iter().find_map(|a| {
            let (key, value) = a.split_once('=')?;
            (key.trim() == "$link")
                .then(|| unquote(value).to_string())
                .filter(|v| !v.is_empty())
        });
        let mut plain = plain.into_iter().map(|a| unquote(&a).to_string());
        if name == "Rel" || name == "BiRel" || name.starts_with("Rel_") {
            let (Some(from), Some(to)) = (plain.next(), plain.next()) else {
                continue;
            };
            let label = plain.next().unwrap_or_default();
            let technology = plain.next().filter(|t| !t.is_empty());
            let rel = C4Rel {
                from: from.clone(),
                to: to.clone(),
                label: label.clone(),
                technology: technology.clone(),
                line,
                text: text.to_string(),
            };
            if name == "BiRel" {
                rels.push(C4Rel {
                    from: to,
                    to: from,
                    ..rel.clone()
                });
            }
            rels.push(rel);
        } else if is_c4_box(name) {
            let Some(alias) = plain.next() else {
                continue;
            };
            boxes.push(C4Box {
                kind: name.to_string(),
                alias,
                label: plain.next().unwrap_or_default(),
                texts: plain.collect(),
                link,
                line,
            });
        }
    }
    (boxes, rels)
}

/// Whether a C4 macro draws an element box (a person, system, container or component).
fn is_c4_box(name: &str) -> bool {
    !name.contains("Boundary")
        && ["Person", "System", "Container", "Component", "Node", "Deployment_Node"]
            .iter()
            .any(|base| name.starts_with(base))
}

/// The arguments of a C4 call, split at commas outside quotes, trimmed.
fn c4_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in inner.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            ',' if !quoted => out.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text)
}

/// The quoted values of `$link="..."` in one diagram line.
fn dollar_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("$link") {
        rest = rest[at + 5..].trim_start();
        let Some(value) = rest
            .strip_prefix('=')
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('"'))
        else {
            continue;
        };
        let Some(close) = value.find('"') else {
            break;
        };
        let target = value[..close].trim();
        if !target.is_empty() {
            out.push(target.to_string());
        }
        rest = &value[close + 1..];
    }
    out
}

/// The longest description a `## Uses` bullet may carry, in words.
pub const USES_MAX_WORDS: usize = 8;

/// One bullet of an item's `## Uses` section: `- [[arch/x]]: verb phrase`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsesBullet {
    /// File line of the bullet.
    pub line: usize,
    /// The link target, with any `#anchor`.
    pub target: String,
    /// The text after the link, without the leading colon and without a trailing `(technology)`;
    /// empty when the bullet has none. A trailing parenthesis that names no remote technology
    /// stays here.
    pub description: String,
    /// How the link leaves the process, from a trailing parenthesised remote technology:
    /// `Reads items (HTTP)`. None for a bullet that stays in the process.
    pub technology: Option<String>,
}

impl UsesBullet {
    /// True when the bullet names a remote technology (HTTP, gRPC, a queue, SQL, a file), so no
    /// code edge between the two components is expected. Anything else counts as in-process.
    pub fn leaves_process(&self) -> bool {
        self.technology.is_some()
    }
}

/// The technology words that mean a link leaves the process, with the tag a `## Uses` bullet
/// gets for each. `uses_bullets` reads it, and the drift check reads what it finds.
const REMOTE: [(&str, &[&str]); 6] = [
    ("gRPC", &["grpc"]),
    ("WebSocket", &["websocket", "websockets"]),
    ("HTTP", &["http", "https", "rest", "restful", "graphql"]),
    ("queue", &["queue", "queues", "amqp", "kafka", "mqtt"]),
    ("SQL", &["sql", "postgres", "postgresql", "mysql", "mssql"]),
    ("file", &["file", "files"]),
];

/// The tag for a technology text that talks over a network, a queue, SQL or a file; None for any
/// other text. A word counts when it stands alone: `tsql` is not `sql`.
pub fn remote_technology(text: &str) -> Option<&'static str> {
    let text = text.to_lowercase();
    REMOTE.iter().find_map(|(tag, words)| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|word| words.contains(&word))
            .then_some(*tag)
    })
}

/// Splits a trailing `(technology)` off a bullet's text when it names a remote technology, or
/// `(over X)` for any X, so a stack the list does not name can still mark a run-time link.
fn split_technology(text: &str) -> (&str, Option<String>) {
    let text = text.trim();
    if let Some(inner) = text.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
        && !inner[open + 1..].contains(')')
    {
        let tech = inner[open + 1..].trim();
        if let Some(stack) = tech
            .strip_prefix("over")
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .map(str::trim)
            .filter(|stack| !stack.is_empty())
        {
            return (inner[..open].trim_end(), Some(stack.to_string()));
        }
        if remote_technology(tech).is_some() {
            return (inner[..open].trim_end(), Some(tech.to_string()));
        }
    }
    (text, None)
}

/// The bullets of the `## Uses` section that open with a `[[link]]`, in order. Fenced blocks and
/// other bullets are skipped.
pub fn uses_bullets(body: &str, first_line: usize) -> Vec<UsesBullet> {
    let Some(section) = sections(body, first_line)
        .into_iter()
        .find(|s| s.title.eq_ignore_ascii_case("uses"))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (line, text, in_fence) in lines(body, first_line) {
        if in_fence || line <= section.start_line || line > section.end_line {
            continue;
        }
        let Some(rest) = text
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| text.trim_start().strip_prefix("* "))
            .map(str::trim)
            .filter(|rest| rest.starts_with("[["))
        else {
            continue;
        };
        let Some((_, target)) = body_links(rest, line).into_iter().next() else {
            continue;
        };
        let after = rest.find("]]").map_or("", |close| &rest[close + 2..]);
        let (description, technology) = split_technology(after.trim().trim_start_matches(':'));
        out.push(UsesBullet {
            line,
            target,
            description: description.to_string(),
            technology,
        });
    }
    out
}
