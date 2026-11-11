//! `renumber`: a new number for a numbered item.

use std::collections::HashSet;

use carvestone_core::{IdScheme, ItemId, LIST_KEYS, NumberedKind, Ref, body_links, diagrams, md_links};
use carvestone_store::{Document, Entry};
use serde::Serialize;
use serde_norway::Value;

use crate::check_links::{RELATION_KEYS, join_target};
use crate::index_file;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// What a renumber did, or for a dry run would do. Paths are as they are after the renumber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Renumbered {
    pub from: String,
    pub to: String,
    pub rewritten: Vec<String>,
    pub review: Vec<String>,
    pub warnings: Vec<String>,
}

/// What to do with one item when `old` is replaced by a new id.
enum Plan {
    Untouched,
    Rewrite(String),
    Review,
}

/// The reference `text` as a `Ref`, when it names exactly `old`; zero padding and case do not matter.
pub(crate) fn points_at(scheme: &IdScheme, text: &str, old: &ItemId) -> Option<Ref> {
    scheme.parse(text.trim()).ok().filter(|r| r.id == *old)
}

/// The canonical text that replaces a reference to `old`, with its anchor kept.
fn swapped(scheme: &IdScheme, text: &str, old: &ItemId, new: &ItemId) -> Option<String> {
    let r = points_at(scheme, text, old)?;
    Some(scheme.link_text(&Ref {
        id: new.clone(),
        anchor: r.anchor,
    }))
}

/// A relation key's values as written: the list, or the one scalar.
fn relation_values(doc: &Document, key: &str) -> Vec<String> {
    if LIST_KEYS.contains(&key) {
        doc.get_list(key).unwrap_or_default()
    } else {
        doc.get_str(key).into_iter().collect()
    }
}

/// True when a relation field, a `[[..]]` body link, a diagram `$link` or a `[x](../tasks/CS-7.md)` link
/// in the item `from` names `old`. `renumber` only reports the markdown links; `rename` rewrites them.
pub(crate) fn refers(scheme: &IdScheme, doc: &Document, from: &ItemId, old: &ItemId) -> bool {
    RELATION_KEYS.iter().any(|key| {
        relation_values(doc, key)
            .iter()
            .any(|v| points_at(scheme, v, old).is_some())
    }) || body_links(&doc.body, doc.body_line())
        .iter()
        .any(|(_, target)| points_at(scheme, target, old).is_some())
        || diagrams(&doc.body, doc.body_line())
            .iter()
            .flat_map(|d| &d.links)
            .any(|(_, target)| points_at(scheme, target, old).is_some())
        || md_links(&doc.body, doc.body_line()).iter().any(|(_, target)| {
            let path = target.split_once('#').map_or(target.as_str(), |(p, _)| p);
            join_target(from.as_str(), path).is_some_and(|id| points_at(scheme, &id, old).is_some())
        })
}

/// Patches only the relation lines that name `old`.
pub(crate) fn rewrite_fields(
    scheme: &IdScheme,
    doc: &mut Document,
    old: &ItemId,
    new: &ItemId,
) -> Result<(), EngineError> {
    for key in RELATION_KEYS {
        let values = relation_values(doc, key);
        let next: Vec<String> = values
            .iter()
            .map(|v| swapped(scheme, v, old, new).unwrap_or_else(|| v.clone()))
            .collect();
        if next == values {
            continue;
        }
        let value = if LIST_KEYS.contains(&key) {
            Value::Sequence(next.into_iter().map(Value::String).collect())
        } else {
            Value::String(next.into_iter().next().unwrap_or_default())
        };
        doc.set(key, value)?;
    }
    Ok(())
}

/// The body with `[[..]]` links and diagram `$link="..."` values naming `old` swapped. Only lines
/// `body_links` and `diagrams` reported are touched, and on a prose line only text outside backtick spans,
/// so other fences and code samples stay as written.
pub(crate) fn rewrite_body(scheme: &IdScheme, body: &str, first_line: usize, old: &ItemId, new: &ItemId) -> String {
    let hits: HashSet<usize> = body_links(body, first_line)
        .into_iter()
        .filter(|(_, target)| points_at(scheme, target, old).is_some())
        .map(|(line, _)| line)
        .collect();
    let boxes: HashSet<usize> = diagrams(body, first_line)
        .into_iter()
        .flat_map(|d| d.links)
        .filter(|(_, target)| points_at(scheme, target, old).is_some())
        .map(|(line, _)| line)
        .collect();
    body.split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            if hits.contains(&(first_line + i)) {
                swap_line(scheme, line, old, new)
            } else if boxes.contains(&(first_line + i)) {
                swap_box_links(scheme, line, old, new)
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// A diagram line with every `$link="old"` value replaced; spacing around the value stays.
fn swap_box_links(scheme: &IdScheme, line: &str, old: &ItemId, new: &ItemId) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("$link") {
        let head = at + "$link".len();
        let after = &rest[head..];
        let quoted = after
            .trim_start()
            .strip_prefix('=')
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('"'));
        let Some(value) = quoted else {
            out.push_str(&rest[..head]);
            rest = after;
            continue;
        };
        let Some(close) = value.find('"') else {
            break;
        };
        let value_at = rest.len() - value.len();
        let inner = &value[..close];
        let lead = inner.len() - inner.trim_start().len();
        let text = inner.trim();
        out.push_str(&rest[..value_at]);
        out.push_str(&inner[..lead]);
        out.push_str(&swapped(scheme, text, old, new).unwrap_or_else(|| text.to_string()));
        out.push_str(&inner[lead + text.len()..]);
        rest = &value[close..];
    }
    out.push_str(rest);
    out
}

fn swap_line(scheme: &IdScheme, line: &str, old: &ItemId, new: &ItemId) -> String {
    line.split('`')
        .enumerate()
        .map(|(n, piece)| {
            if n % 2 == 0 {
                swap_links(scheme, piece, old, new)
            } else {
                piece.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

fn swap_links(scheme: &IdScheme, text: &str, old: &ItemId, new: &ItemId) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else {
            break;
        };
        let inner = &after[..close];
        out.push_str(&rest[..open + 2]);
        out.push_str(&swapped(scheme, inner, old, new).unwrap_or_else(|| inner.to_string()));
        out.push_str("]]");
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    out
}

/// Decides one item. Nothing is written here.
fn plan_entry(scheme: &IdScheme, entry: &Entry, old: &ItemId, new: &ItemId) -> Plan {
    let Ok(doc) = &entry.doc else {
        return Plan::Review;
    };
    if !refers(scheme, doc, &entry.id, old) {
        return Plan::Untouched;
    }
    if !doc.subset_problems().is_empty() {
        return Plan::Review;
    }
    let mut next = doc.clone();
    if rewrite_fields(scheme, &mut next, old, new).is_err() {
        return Plan::Review;
    }
    next.body = rewrite_body(scheme, &doc.body, doc.body_line(), old, new);
    // A reference the line rewrite could not reach is left for a person, not half done.
    if refers(scheme, &next, &entry.id, old) {
        return Plan::Review;
    }
    let text = next.to_text();
    if text == entry.text {
        Plan::Untouched
    } else {
        Plan::Rewrite(text)
    }
}

/// The entry whose store path is `input`, taken as given and never through `IdScheme::parse`.
fn find_target<'a>(snapshot: &'a Snapshot, input: &str) -> Result<&'a Entry, EngineError> {
    let cleaned = input.trim().replace('\\', "/");
    let cleaned = cleaned.trim_start_matches("./");
    let cleaned = cleaned.strip_prefix("stone/").unwrap_or(cleaned);
    let id = ItemId::from_path(cleaned);
    snapshot
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| EngineError::NotFound {
            id: input.trim().to_string(),
            suggestion: None,
        })
}

/// The id a dry run shows in place of the number it cannot allocate without the lock.
fn placeholder(scheme: &IdScheme, kind: NumberedKind) -> ItemId {
    let sample = scheme.numbered_id(kind, 1);
    let stem = sample.as_str().strip_suffix('1').unwrap_or(sample.as_str());
    ItemId::from_path(&format!("{stem}<next>"))
}

impl Repo {
    /// True when the file on disk is valid UTF-8; the scan decodes lossily and saving that text would damage the file.
    pub(crate) fn readable(&self, entry: &Entry) -> bool {
        std::fs::read(self.store.layout.item_path(&entry.id)).is_ok_and(|bytes| std::str::from_utf8(&bytes).is_ok())
    }

    /// Gives the numbered item at `path` the next free number and renames its file.
    pub fn renumber(&self, path: &str, dry_run: bool) -> Result<Renumbered, EngineError> {
        self.refuse_doc_file(path, false)?;
        // The lock is taken before anything is read and held to the end of this function, so the number,
        // the rewrites and the rename all see the same store. A dry run takes none.
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        let snapshot = self.snapshot()?;
        let scheme = &snapshot.scheme;
        let target = find_target(&snapshot, path)?;
        let (kind, n) = scheme.number(&target.id).ok_or_else(|| {
            EngineError::Invalid(format!(
                "{} is not a numbered item; only tasks, milestones and decisions have numbers",
                target.path
            ))
        })?;
        let old = scheme.numbered_id(kind, n);
        let collision = snapshot
            .entries
            .iter()
            .any(|e| e.id != target.id && scheme.number(&e.id) == Some((kind, n)));

        let (new, warnings) = match &lock {
            Some(lock) => {
                let allocation = self.store.next_number(lock, kind)?;
                (
                    scheme.numbered_id(kind, allocation.number),
                    allocation.warning.into_iter().collect(),
                )
            }
            None => (placeholder(scheme, kind), Vec::new()),
        };
        let to = format!("stone/{new}.md");
        let after = |entry: &Entry| {
            if entry.id == target.id {
                to.clone()
            } else {
                entry.path.clone()
            }
        };

        let mut writes: Vec<(&Entry, String)> = Vec::new();
        let mut review: Vec<String> = Vec::new();
        for entry in &snapshot.entries {
            if collision {
                // The old id still names the other file, so no link can be told apart: leave them all.
                let linked = match &entry.doc {
                    Ok(doc) => refers(scheme, doc, &entry.id, &old),
                    Err(_) => true,
                };
                if linked {
                    review.push(after(entry));
                }
                continue;
            }
            match plan_entry(scheme, entry, &old, &new) {
                Plan::Untouched => {}
                Plan::Review => review.push(after(entry)),
                Plan::Rewrite(text) if self.readable(entry) => writes.push((entry, text)),
                Plan::Rewrite(_) => review.push(after(entry)),
            }
        }
        let mut rewritten: Vec<String> = writes.iter().map(|(entry, _)| after(entry)).collect();
        rewritten.sort();
        review.sort();

        if let Some(lock) = &lock {
            // Links first, rename last. A stop in between leaves links to a number no file has yet and the file
            // under its old name, and running renumber again finishes the job. Renaming first would leave the
            // links pointing at nothing with no old name left to renumber.
            for (entry, text) in &writes {
                self.store.save(lock, &entry.id, text)?;
            }
            self.store.rename(lock, &target.id, &new).map_err(|e| {
                EngineError::Invalid(format!(
                    "{e}; links already use {}; run `stone renumber {}` again",
                    scheme.short(&new).unwrap_or_else(|| new.to_string()),
                    target.path
                ))
            })?;
            self.sync_index(lock, |index| {
                let index = index_file::drop_line(index, &target.id);
                match &target.item {
                    Some(item) => index_file::put(&index, scheme, &new, item),
                    None => index,
                }
            })?;
        }
        Ok(Renumbered {
            from: target.path.clone(),
            to,
            rewritten,
            review,
            warnings,
        })
    }
}
