//! The plain-text shapes of `get`, `list`, `search` and `code`. They print what an
//! agent acts on and nothing it can read off elsewhere: no constant columns, no
//! default values, one file header for the symbols under it. `--json`, MCP and
//! HTTP keep the full engine data.

use std::collections::{HashMap, HashSet};

use carvestone_engine::code_entries::EntryRow;
use carvestone_engine::query::ListRow;
use carvestone_engine::search::Hit;
use carvestone_engine::tiers::{self, Part};

/// Tokens as `Hit::cost` counts them: chars / 4, rounded up.
pub fn tokens(text: &str) -> usize {
    tiers::tokens(text)
}

/// Renders `count` items so everything printed fits `budget`: the largest prefix
/// whose text, cut-off notice included, costs at most the budget. One item always stays.
pub fn fit(budget: Option<usize>, count: usize, render: impl Fn(usize) -> String) -> String {
    let Some(budget) = budget else {
        return render(count);
    };
    for kept in (1..=count).rev() {
        let text = render(kept);
        if tokens(&text) <= budget || kept == 1 {
            return text;
        }
    }
    render(0)
}

/// Cuts printed text by whole lines so it fits `budget` tokens: the first line never goes, the rest
/// shrinks, and one last line names how many lines and how many tokens were left out. An answer the
/// budget does not cut is returned byte for byte.
pub fn fit_lines(budget: Option<usize>, text: &str) -> String {
    let Some(budget) = budget else {
        return text.to_string();
    };
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return text.to_string();
    };
    let parts = vec![
        Part::critical("first line", vec![first.to_string()]),
        Part::optional("the rest", None, lines.map(str::to_string).collect()),
    ];
    let fitted = tiers::fit(&parts, Some(budget));
    if fitted.left_out.is_empty() {
        text.to_string()
    } else {
        fitted.text
    }
}

/// `tasks/CS-1` is `CS-1` everywhere an agent types it; other ids print as they are.
pub fn short_id(id: &str) -> String {
    match id.split_once('/') {
        Some(("tasks" | "decisions" | "milestones", rest)) if !rest.contains('/') => rest.to_string(),
        _ => id.to_string(),
    }
}

/// How many leading words of a code hit's snippet are detail (`@L5`, `depth 2`, `in 7`) before the signature.
#[derive(Clone, Copy)]
pub enum Detail {
    /// The snippet is the signature.
    None,
    /// `@L5 signature`, or `calls signature` on a path step.
    One,
    /// `depth 2 signature` or `in 7 signature`.
    Two,
}

fn split_detail(snippet: &str, detail: Detail) -> (&str, &str) {
    let words = match detail {
        Detail::None => 0,
        Detail::One => 1,
        Detail::Two => 2,
    };
    let snippet = snippet.trim();
    let mut rest = snippet;
    for _ in 0..words {
        let Some(at) = rest.find(char::is_whitespace) else {
            return (snippet, "");
        };
        rest = rest[at..].trim_start();
    }
    (snippet[..snippet.len() - rest.len()].trim_end(), rest)
}

/// Code hits under their file: one header per file, then `id  L<a>-<b>  <kind>  <detail> <signature>`.
/// `inferred` marks the non-default edge; a symbol met again prints no second signature.
/// With `group`, a file's hits sit together in order of first appearance; without it only neighbours merge.
pub fn symbol_lines(hits: &[Hit], detail: Detail, group: bool, seen: &mut HashSet<String>) -> String {
    let mut groups: Vec<(&str, Vec<&Hit>)> = Vec::new();
    for hit in hits {
        let slot = if group {
            groups.iter().position(|(path, _)| *path == hit.path)
        } else {
            groups.len().checked_sub(1).filter(|last| groups[*last].0 == hit.path)
        };
        match slot {
            Some(at) => groups[at].1.push(hit),
            None => groups.push((&hit.path, vec![hit])),
        }
    }
    let mut out = String::new();
    for (path, members) in groups {
        out.push_str(path);
        out.push('\n');
        for hit in members {
            let mut parts = vec![
                hit.id.clone(),
                format!("L{}-{}", hit.lines[0], hit.lines[1]),
                hit.kind.clone(),
            ];
            if hit.exactness.as_deref() == Some("inferred") {
                parts.push("inferred".to_string());
            }
            let (lead, signature) = split_detail(&hit.snippet, detail);
            let first = seen.insert(hit.id.clone());
            let rest = if first || signature.is_empty() {
                hit.snippet.trim().to_string()
            } else {
                lead.trim().to_string()
            };
            if !rest.is_empty() {
                parts.push(rest);
            }
            out.push_str("  ");
            out.push_str(&parts.join("  "));
            out.push('\n');
        }
    }
    out
}

/// One entry row: `target  kind  detail`, with `declared` after a kind the front matter names. The
/// `declared` kind (not seen by detection) and `gone` stand alone.
pub fn entry_line(row: &EntryRow) -> String {
    format!("{}\n", row.line())
}

/// `explain`'s file rows: `path  L1-<n>`, the language once in the heading when most files share it,
/// and only on the rows that differ.
pub fn file_lines(files: &[Hit]) -> (String, String) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for file in files {
        *counts.entry(&file.snippet).or_default() += 1;
    }
    let common = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(lang, _)| (*lang).to_string())
        .unwrap_or_default();
    let mut body = String::new();
    for file in files {
        body.push_str(&format!("{}  L{}-{}", file.path, file.lines[0], file.lines[1]));
        if file.snippet != common {
            body.push_str(&format!("  {}", file.snippet));
        }
        body.push('\n');
    }
    (common, body)
}

/// Whether a search hit is an item's front matter whose text is just its title.
fn title_only(hit: &Hit, titles: &HashMap<String, String>) -> bool {
    if hit.anchor.is_some() || hit.exactness.is_some() || hit.kind == "symbol" {
        return false;
    }
    let Some(title) = titles.get(&hit.id) else {
        return false;
    };
    let shown = hit.snippet.trim_end_matches('…');
    hit.snippet == *title || (hit.snippet.ends_with('…') && title.starts_with(shown))
}

/// Search hits as lines: `CS-1#anchor  L<a>-<b>  <kind>  <snippet>`. A title-only hit is dropped when
/// its item has another hit; an item that matched only there keeps one hit without the snippet.
pub fn search_lines(hits: &[Hit], titles: &HashMap<String, String>) -> Vec<String> {
    let has_text: HashSet<&str> = hits
        .iter()
        .filter(|hit| !title_only(hit, titles))
        .map(|hit| hit.id.as_str())
        .collect();
    let mut kept_title: HashSet<&str> = HashSet::new();
    let mut lines = Vec::new();
    for hit in hits {
        let only = title_only(hit, titles);
        if only && (has_text.contains(hit.id.as_str()) || !kept_title.insert(hit.id.as_str())) {
            continue;
        }
        let anchor = hit.anchor.as_ref().map_or(String::new(), |a| format!("#{a}"));
        let snippet = if only { "" } else { hit.snippet.as_str() };
        let line = format!(
            "{}{anchor}  L{}-{}  {}  {snippet}",
            short_id(&hit.id),
            hit.lines[0],
            hit.lines[1],
            hit.kind
        );
        lines.push(line.trim_end().to_string());
    }
    lines
}

/// List rows without the columns the command line already fixed and without default values.
pub fn list_lines(rows: &[ListRow], type_known: bool, status_known: bool) -> String {
    let mut out = String::new();
    for row in rows {
        let mut cols: Vec<&str> = vec![row.short.as_deref().unwrap_or(&row.id)];
        if !type_known {
            cols.push(&row.type_name);
        }
        if !status_known {
            cols.push(row.status.as_deref().unwrap_or("-"));
        }
        if let Some(priority) = row.priority.as_deref().filter(|p| *p != "medium") {
            cols.push(priority);
        }
        if let Some(title) = row.title.as_deref() {
            cols.push(title);
        }
        out.push_str(&cols.join("  "));
        out.push('\n');
    }
    out
}

/// An item file for reading: front matter without `type`, the default priority and dates
/// that repeat `created`, then the body.
pub fn item_text(text: &str) -> String {
    let Some((front, body)) = split_front_matter(text) else {
        return with_newline(text.trim_end());
    };
    let mut entries: Vec<Vec<&str>> = Vec::new();
    for line in front {
        let continues = line.starts_with(char::is_whitespace) || line.starts_with('-');
        match entries.last_mut() {
            Some(entry) if continues => entry.push(line),
            _ => entries.push(vec![line]),
        }
    }
    let value_of = |entry: &[&str]| -> Option<(String, String)> {
        let (key, value) = entry.first()?.split_once(':')?;
        Some((
            key.trim().to_string(),
            value.trim().trim_matches(['\'', '"']).to_string(),
        ))
    };
    let created = entries
        .iter()
        .filter_map(|entry| value_of(entry))
        .find(|(key, _)| key == "created")
        .map(|(_, value)| value);
    let mut out = String::new();
    let mut kept = String::new();
    for entry in &entries {
        let Some((key, value)) = value_of(entry) else {
            continue;
        };
        let repeats_created = created.as_deref().is_some_and(|date| {
            // A timestamp repeats the day it was created on, so compare days, not whole values.
            let day = date.get(..10).unwrap_or(date);
            let same_day = value == day || value.strip_prefix(day).is_some_and(|rest| rest.starts_with('T'));
            key != "created"
                && entry.len() == 1
                && (same_day
                    || (key == "generated"
                        && (value.contains(&format!("at: {day}")) || value.contains(&format!("at: '{day}")))))
        });
        if key == "type" || (key == "priority" && value == "medium") || repeats_created {
            continue;
        }
        for line in entry {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    if !kept.is_empty() {
        out.push_str("---\n");
        out.push_str(&kept);
        out.push_str("---\n");
    }
    out.push_str(body.trim());
    with_newline(&out)
}

/// The front matter lines between the opening fence and the closing one, and the text after it.
fn split_front_matter(text: &str) -> Option<(Vec<&str>, &str)> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut front = Vec::new();
    let mut used = first.len();
    for line in lines {
        used += line.len();
        if line.trim_end() == "---" {
            return Some((front, text.get(used..).unwrap_or_default()));
        }
        front.push(line.trim_end_matches(['\r', '\n']));
    }
    None
}

/// `text` with trailing blank space gone and one final newline.
pub fn with_newline(text: &str) -> String {
    format!("{}\n", text.trim_end())
}

#[cfg(test)]
mod tests {
    use super::item_text;

    #[test]
    fn item_text_hides_dates_of_the_created_day() {
        let cases = [
            (
                "plain days",
                "created: 2026-11-01",
                "generated: { by: claude-code, at: 2026-11-01 }",
                false,
            ),
            (
                "timestamps of one day",
                "created: 2026-11-01T10:00:00+00:00",
                "generated: { by: claude-code, at: '2026-11-01T10:00:05+00:00' }",
                false,
            ),
            (
                "plain created, timestamp generated",
                "created: 2026-11-01",
                "generated: { by: claude-code, at: '2026-11-01T18:00:00+00:00' }",
                false,
            ),
            (
                "another day",
                "created: 2026-11-01T10:00:00+00:00",
                "generated: { by: claude-code, at: '2026-11-02T09:00:00+00:00' }",
                true,
            ),
        ];
        for (what, created, generated, shown) in cases {
            let text = format!("---\ntitle: A\n{created}\n{generated}\n---\n\nBody.\n");
            let out = item_text(&text);
            assert_eq!(out.contains("generated"), shown, "{what}: {out}");
            assert!(out.contains(created), "{what}: {out}");
        }
    }
}
