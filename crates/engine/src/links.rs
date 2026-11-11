//! Link rows for the index: relation fields, `[[wikilinks]]`, markdown links
//! to `.md` files in `stone/`, `[[code:...]]` and the `$link`s of C4 diagram boxes. Targets are stored as
//! canonical ids when they parse, as written otherwise; `stone check` is what
//! reports the unresolved ones.

use carvestone_core::{IdScheme, body_links, diagrams, md_links};
use carvestone_store::{Config, Entry};

use crate::check_links::{RELATION_KEYS, link_id, relation_values};
use crate::units::Unit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRow {
    /// The anchor of the section the link is in; empty for the lead unit.
    pub src_anchor: String,
    pub dst_item: String,
    pub dst_anchor: Option<String>,
    pub kind: String,
    pub line: u32,
}

/// What reading links needs from the config, built once per refresh instead of per file.
pub struct LinkContext {
    scheme: IdScheme,
    config: Config,
}

impl LinkContext {
    pub fn new(config: &Config) -> LinkContext {
        LinkContext {
            scheme: config.scheme(),
            config: config.clone(),
        }
    }
}

/// Every link of an entry, in file order. Links in code fences and code spans
/// are skipped by the core scanners. A doc-root file has no relation fields, so only its
/// body links count.
pub fn links_of(ctx: &LinkContext, entry: &Entry, units: &[Unit]) -> Vec<LinkRow> {
    let Some(item) = &entry.item else {
        return Vec::new();
    };
    let LinkContext { scheme, config } = ctx;
    let mut out = Vec::new();
    let mut push = |line: usize, kind: &str, dst_item: String, dst_anchor: Option<String>| {
        out.push(LinkRow {
            src_anchor: unit_at(units, line),
            dst_item,
            dst_anchor,
            kind: kind.to_string(),
            line: line as u32,
        });
    };

    for key in RELATION_KEYS {
        let line = entry.doc.as_ref().ok().and_then(|doc| doc.key_line(key)).unwrap_or(1);
        for value in relation_values(&item.fields, key) {
            let value = value.trim();
            if !value.is_empty() {
                let (dst, anchor) = canonical(scheme, value);
                push(line, key, dst, anchor);
            }
        }
    }
    for (line, target) in body_links(&item.body, item.body_line) {
        if target.starts_with("code:") {
            push(line, "code", target, None);
        } else {
            let (dst, anchor) = canonical(scheme, &target);
            push(line, "wikilink", dst, anchor);
        }
    }
    for (line, target) in md_links(&item.body, item.body_line) {
        let (path, anchor) = match target.split_once('#') {
            Some((p, a)) => (p, Some(a.to_string())),
            None => (target.as_str(), None),
        };
        match link_id(Some(config), &entry.path, path) {
            Some(id) => push(line, "md", id, anchor),
            None => push(line, "md", target, None),
        }
    }
    for diagram in diagrams(&item.body, item.body_line) {
        for (line, target) in diagram.links {
            let (dst, anchor) = canonical(scheme, &target);
            push(line, "diagram", dst, anchor);
        }
    }
    out
}

fn canonical(scheme: &IdScheme, text: &str) -> (String, Option<String>) {
    match scheme.parse(text) {
        Ok(r) => (r.id.as_str().to_string(), r.anchor),
        Err(_) => (text.to_string(), None),
    }
}

/// The unit holding `line`; the lead unit (empty anchor) covers the front matter.
fn unit_at(units: &[Unit], line: usize) -> String {
    units
        .iter()
        .rev()
        .find(|u| u.line_start as usize <= line && line <= u.line_end as usize)
        .map(|u| u.anchor.clone())
        .unwrap_or_default()
}
