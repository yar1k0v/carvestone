//! `stone task check` and the gate of `stone task move <id> in-progress`: what a task still lacks
//! before a Developer can start it, worded as one plain sentence per gap.

use std::collections::HashSet;

use carvestone_core::{ItemType, checkboxes, code_span_ranges, lines, sections};
use serde::Serialize;

use crate::code::open;
use crate::code_entries::{EntryState, entry_state, has_code};
use crate::design::span_target;
use crate::edit::type_of;
use crate::repo::{EngineError, Repo};

/// The answer of `stone task check`: no gaps means the task is ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ready {
    pub id: String,
    pub gaps: Vec<String>,
}

/// A code span of the plan: its file line, its text without backticks, and whether `(new)` follows it.
pub(crate) struct Span {
    pub(crate) line: usize,
    pub(crate) text: String,
    pub(crate) marked: bool,
}

/// True for a line that opens a numbered step: digits, a dot and a space.
fn numbered(text: &str) -> bool {
    let text = text.trim_start();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && text[digits..].starts_with(". ")
}

pub(crate) fn path_of(target: &str) -> &str {
    target.split('#').next().unwrap_or("").trim()
}

/// The lines of the task's Plan section outside fences, with their file lines.
pub(crate) fn plan_lines(item: &carvestone_core::Item) -> Vec<(usize, &str)> {
    let secs = sections(&item.body, item.body_line);
    secs.iter()
        .find(|s| s.title.eq_ignore_ascii_case("Plan"))
        .map(|plan| {
            lines(&item.body, item.body_line)
                .into_iter()
                .filter(|(line, _, fenced)| !fenced && *line > plan.start_line && *line <= plan.end_line)
                .map(|(line, text, _)| (line, text))
                .collect()
        })
        .unwrap_or_default()
}

/// Every code span of the plan lines, in plan order.
pub(crate) fn plan_spans(plan_lines: &[(usize, &str)]) -> Vec<Span> {
    let mut spans = Vec::new();
    for (line, text) in plan_lines {
        for (start, end) in code_span_ranges(text) {
            spans.push(Span {
                line: *line,
                text: text[start..end].trim_matches('`').trim().to_string(),
                marked: text[end..].trim_start().starts_with("(new)"),
            });
        }
    }
    spans
}

/// Every gap of the task, in the order a Lead fixes them. A file or symbol the plan adds is written
/// as a code span followed by `(new)`; it is not looked up, because it cannot exist yet.
pub(crate) fn gaps(repo: &Repo, id: &str, item: &carvestone_core::Item) -> Result<Vec<String>, EngineError> {
    let mut out = Vec::new();
    if item.fields.components.is_empty() {
        out.push(format!(
            "components is empty; set it with `stone edit {id} --set components=<arch id>`"
        ));
    }
    if item.fields.paths.is_empty() {
        out.push(format!(
            "paths is empty; set it with `stone edit {id} --set paths=<file>,<file>`"
        ));
    }

    let secs = sections(&item.body, item.body_line);
    let section = |name: &str| secs.iter().find(|s| s.title.eq_ignore_ascii_case(name));

    let plan_lines = plan_lines(item);
    let spans = plan_spans(&plan_lines);
    let marked: HashSet<&str> = spans.iter().filter(|s| s.marked).map(|s| s.text.as_str()).collect();

    let root = &repo.store.layout.root;
    for path in &item.fields.paths {
        if !root.join(path).exists() && !marked.contains(path.as_str()) {
            out.push(format!(
                "path {path} does not exist; fix it in paths, or write it in the plan as a code span followed by (new)"
            ));
        }
    }

    let criteria = section("Acceptance criteria")
        .map(|s| checkboxes(&item.body, item.body_line, s))
        .unwrap_or_default();
    if criteria.is_empty() {
        out.push("no acceptance criterion; add `- [ ]` lines under ## Acceptance criteria".to_string());
    }
    for criterion in &criteria {
        if code_span_ranges(&criterion.text).is_empty() {
            out.push(format!(
                "criterion {} carries no command; put the command in a code span",
                criterion.n
            ));
        }
    }

    if !plan_lines.iter().any(|(_, text)| numbered(text)) {
        out.push("the plan has no numbered steps; write them under ## Plan".to_string());
    }

    let pending: Vec<(usize, String)> = spans
        .iter()
        .filter(|s| !s.marked)
        .filter_map(|s| span_target(&s.text).map(|target| (s.line, target)))
        .filter(|(_, target)| !marked.contains(target.as_str()) && !marked.contains(path_of(target)))
        .collect();
    // The index is only opened (and brought up to date) when the plan names something to look up.
    if !pending.is_empty() {
        let index = open(repo)?;
        let conn = index.conn();
        if has_code(conn)? {
            for (line, target) in pending {
                match entry_state(conn, &target)? {
                    EntryState::Found => {}
                    // A file that exists but is not code (a manifest, a doc) is not judged.
                    EntryState::NoFile if root.join(path_of(&target)).is_file() => {}
                    EntryState::NoFile => out.push(format!(
                        "plan line {line}: {target} names a file the code graph does not hold; fix the path or mark it (new)"
                    )),
                    EntryState::NoSymbol => out.push(format!(
                        "plan line {line}: {target} names a symbol that is not in that file; fix the name or mark it (new)"
                    )),
                }
            }
        }
    }
    Ok(out)
}

/// Refuses with one sentence naming every gap of `item`, and `override_command` as the way past them.
pub(crate) fn refuse_unready(
    repo: &Repo,
    label: &str,
    item: &carvestone_core::Item,
    override_command: &str,
) -> Result<(), EngineError> {
    let gaps = gaps(repo, label, item)?;
    if gaps.is_empty() {
        return Ok(());
    }
    Err(EngineError::Invalid(format!(
        "{label} is not ready for in-progress: {}; fix them, or run {override_command}",
        gaps.join("; ")
    )))
}

impl Repo {
    /// What the task still lacks before a Developer can start it; no gaps means it is ready.
    pub fn task_check(&self, input: &str) -> Result<Ready, EngineError> {
        let snapshot = self.snapshot()?;
        let (entry, _) = snapshot.resolve(input)?;
        let id = snapshot.label(entry);
        let item = match &entry.item {
            Some(item) if type_of(entry)? == ItemType::Task => item,
            _ => return Err(EngineError::NotATask { id }),
        };
        let gaps = gaps(self, &id, item)?;
        Ok(Ready { id, gaps })
    }
}
