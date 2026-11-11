//! Section units: what search indexes. Unlike `section::sections`, whose ranges
//! include subsections, a unit runs from its heading to the next heading of any
//! level, so units never overlap and BM25 counts each line once.

use carvestone_core::item::Item;
use carvestone_core::section::{self, Section};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// Empty for the lead unit.
    pub anchor: String,
    pub title: String,
    /// 0 for the lead unit.
    pub level: u8,
    pub line_start: u32,
    pub line_end: u32,
    pub text: String,
}

/// The lead unit first, then one unit per heading in file order.
pub fn units_of(item: &Item) -> Vec<Unit> {
    let secs = section::sections(&item.body, item.body_line);
    let body_lines = item.body.lines().count();
    let body_end = (item.body_line + body_lines).saturating_sub(1);

    let lead_end = secs.first().map_or(body_end, |s| s.start_line - 1);
    let mut units = vec![lead(item, lead_end)];

    for (i, sec) in secs.iter().enumerate() {
        let end_line = secs.get(i + 1).map_or(sec.end_line, |next| next.start_line - 1);
        let flat = Section {
            end_line,
            ..sec.clone()
        };
        units.push(Unit {
            anchor: sec.anchor.clone(),
            title: sec.title.clone(),
            level: sec.level,
            line_start: sec.start_line as u32,
            line_end: end_line as u32,
            text: section::section_text(&item.body, item.body_line, &flat).to_string(),
        });
    }
    units
}

fn lead(item: &Item, end_line: usize) -> Unit {
    let fields = &item.fields;
    let before = end_line.saturating_sub(item.body_line.saturating_sub(1));
    let intro: String = item.body.split_inclusive('\n').take(before).collect();
    let mut parts = Vec::new();
    if let Some(description) = &fields.description {
        parts.push(description.trim().to_string());
    }
    if !fields.tags.is_empty() {
        parts.push(fields.tags.join(" "));
    }
    if !intro.trim().is_empty() {
        parts.push(intro.trim().to_string());
    }
    Unit {
        anchor: String::new(),
        title: fields.title.clone().unwrap_or_default(),
        level: 0,
        line_start: 1,
        line_end: end_line.max(1) as u32,
        text: parts.join("\n"),
    }
}
