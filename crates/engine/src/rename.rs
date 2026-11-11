//! `stone rename`: a new id for a slug item (doc, plan, reference, architecture item), with every link to it
//! rewritten in the same write. A numbered item keeps its number for life (ADR-4).

use carvestone_core::{IdScheme, ItemId, Ref, links};
use carvestone_store::Entry;
use serde::Serialize;

use crate::index_file;
use crate::renumber::{refers, rewrite_body, rewrite_fields};
use crate::repo::{EngineError, Repo};

/// What a rename did, or for a dry run would do. Paths are as they are after the rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Renamed {
    pub from: String,
    pub to: String,
    /// False when the item already has the new id and nothing was written.
    pub changed: bool,
    pub rewritten: Vec<String>,
    pub review: Vec<String>,
}

/// What to do with one item when `old` becomes `new`.
enum Plan {
    Untouched,
    Rewrite(String),
    Review,
}

/// The top folder of an item id: `docs` for `docs/sub/x`.
fn folder_of(id: &ItemId) -> &str {
    id.as_str().split('/').next().unwrap_or("")
}

/// The path parts of a store file: `stone/docs/x.md` is `["stone", "docs", "x.md"]`.
pub(crate) fn parts(path: &str) -> Vec<String> {
    path.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

pub(crate) fn dir_of(file: &[String]) -> &[String] {
    &file[..file.len().saturating_sub(1)]
}

/// The root-relative parts a relative link `path` reaches from the folder `dir`; `None` when it climbs out of the root.
fn resolve(dir: &[String], path: &str) -> Option<Vec<String>> {
    let mut out = dir.to_vec();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other.to_string()),
        }
    }
    Some(out)
}

/// The relative link from the folder `dir` to the file `to`.
pub(crate) fn relative(dir: &[String], to: &[String]) -> String {
    let to_dir = dir_of(to);
    let common = dir.iter().zip(to_dir).take_while(|(a, b)| a == b).count();
    let mut out = "../".repeat(dir.len() - common);
    out.push_str(&to[common..].join("/"));
    out
}

/// The body with every `[x](path.md)` link that points at the renamed file, or that the move of this very file
/// breaks, pointed at the new place. Only the path text changes; an `#anchor` and the link text stay.
fn rewrite_md(body: &str, first_line: usize, from: (&[String], &[String]), file: (&[String], &[String])) -> String {
    let ((dir_before, dir_after), (old, new)) = (from, file);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for link in links(body, first_line).into_iter().filter(|l| l.is_relative_md()) {
        let path = link.target.split_once('#').map_or(link.target.as_str(), |(p, _)| p);
        let Some(target) = resolve(dir_before, path) else {
            continue;
        };
        let fresh = if target == old {
            if dir_before == dir_after && dir_of(old) == dir_of(new) {
                // Same folder: keep how the link was written and swap the file name.
                let prefix = path.rsplit_once('/').map_or("", |(p, _)| &path[..p.len() + 1]);
                format!("{prefix}{}", new.last().map_or("", String::as_str))
            } else {
                relative(dir_after, new)
            }
        } else if dir_before != dir_after {
            relative(dir_after, &target)
        } else {
            continue;
        };
        if fresh != path {
            edits.push((link.start, link.start + path.len(), fresh));
        }
    }
    let mut out = body.to_string();
    for (start, end, text) in edits.into_iter().rev() {
        out.replace_range(start..end, &text);
    }
    out
}

/// Decides one item. Nothing is written here.
fn plan_entry(scheme: &IdScheme, entry: &Entry, old: &ItemId, new: &ItemId) -> Plan {
    let Ok(doc) = &entry.doc else {
        return Plan::Review;
    };
    let moves = entry.id == *old;
    let here = if moves { new } else { &entry.id };
    let linked = refers(scheme, doc, &entry.id, old);
    let (file_old, file_new) = (parts(&format!("stone/{old}.md")), parts(&format!("stone/{new}.md")));
    let dir_before = dir_of(&parts(&entry.path)).to_vec();
    let dir_after = if moves {
        dir_of(&file_new).to_vec()
    } else {
        dir_before.clone()
    };
    if !linked && dir_before == dir_after {
        return Plan::Untouched;
    }
    if linked && !doc.subset_problems().is_empty() {
        return Plan::Review;
    }
    let mut next = doc.clone();
    if rewrite_fields(scheme, &mut next, old, new).is_err() {
        return Plan::Review;
    }
    let body = rewrite_body(scheme, &doc.body, doc.body_line(), old, new);
    next.body = rewrite_md(
        &body,
        doc.body_line(),
        (&dir_before, &dir_after),
        (&file_old, &file_new),
    );
    // A reference the rewrite could not reach is left for a person, not half done.
    if refers(scheme, &next, here, old) {
        return Plan::Review;
    }
    let text = next.to_text();
    if text == entry.text {
        Plan::Untouched
    } else {
        Plan::Rewrite(text)
    }
}

/// What a person typed as the new id, checked: a free slug id in the folder the item is already in.
fn new_id(scheme: &IdScheme, input: &str, target: &Entry, entries: &[Entry]) -> Result<ItemId, EngineError> {
    let folder = folder_of(&target.id);
    let shown = input.trim();
    let parsed: Ref = scheme.parse(shown).map_err(|_| {
        EngineError::Invalid(format!(
            "`{shown}` is not an id this item can have; write it as {folder}/<name>, using letters, digits, `.`, `_` and `-` (and `/` for a subfolder)"
        ))
    })?;
    if parsed.anchor.is_some() {
        return Err(EngineError::Invalid(format!(
            "`{shown}` names a section; give the new id without the `#` part"
        )));
    }
    let id = parsed.id;
    if folder_of(&id) != folder {
        return Err(EngineError::Invalid(format!(
            "`{shown}` is outside {folder}/; a rename keeps the item in its folder, so write the new id as {folder}/<name>"
        )));
    }
    if let Some(taken) = entries
        .iter()
        .find(|e| e.id != target.id && e.id.as_str().eq_ignore_ascii_case(id.as_str()))
    {
        return Err(EngineError::Invalid(format!(
            "`{shown}` is taken by {}; pick another id, or delete that item first",
            taken.path
        )));
    }
    Ok(id)
}

impl Repo {
    /// Gives the slug item `input` the id `new_input` and points every link at it to the new id. A dry run
    /// takes no lock and writes nothing. Naming an item that already has the new id changes nothing.
    pub fn rename_item(&self, input: &str, new_input: &str, dry_run: bool) -> Result<Renamed, EngineError> {
        self.refuse_doc_file(input, false)?;
        // The lock is taken before anything is read and held to the end, so the checks, the rewrites and the
        // move all see the same store.
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        let snapshot = self.snapshot()?;
        let scheme = &snapshot.scheme;
        let target = match snapshot.resolve(input) {
            Ok((entry, None)) => entry,
            Ok((_, Some(_))) => {
                return Err(EngineError::Invalid(format!(
                    "`{input}` names a section; pass the item id to rename the whole item"
                )));
            }
            Err(err) => {
                // A rerun after the rename: the old id is gone and the new one is there.
                if let (Ok(old), Ok(new)) = (scheme.parse(input.trim()), scheme.parse(new_input.trim()))
                    && old.anchor.is_none()
                    && new.anchor.is_none()
                    && snapshot.entries.iter().any(|e| e.id == new.id)
                    && !snapshot.entries.iter().any(|e| e.id == old.id)
                {
                    let path = |id: &ItemId| format!("stone/{id}.md");
                    return Ok(Renamed {
                        from: path(&old.id),
                        to: path(&new.id),
                        changed: false,
                        rewritten: Vec::new(),
                        review: Vec::new(),
                    });
                }
                return Err(self.refuse_doc_file(input, true).err().unwrap_or(err));
            }
        };
        let label = snapshot.label(target);
        if scheme.number(&target.id).is_some() || matches!(folder_of(&target.id), "tasks" | "milestones" | "decisions")
        {
            return Err(EngineError::Invalid(format!(
                "{label} has a number, and a numbered item is never renamed; {}",
                if folder_of(&target.id) == "tasks" {
                    format!(
                        "remove a task made by mistake with `stone task remove {label}`, or close one that was real work with `stone task move {label} archived`"
                    )
                } else {
                    format!("delete a mistaken one with `stone delete {label}`, or close it by changing its status")
                }
            )));
        }
        let new = new_id(scheme, new_input, target, &snapshot.entries)?;
        let old = target.id.clone();
        let (from, to) = (target.path.clone(), format!("stone/{new}.md"));
        if new == old {
            return Ok(Renamed {
                from,
                to,
                changed: false,
                rewritten: Vec::new(),
                review: Vec::new(),
            });
        }
        let after = |entry: &Entry| {
            if entry.id == old {
                to.clone()
            } else {
                entry.path.clone()
            }
        };

        let mut writes: Vec<(&Entry, String)> = Vec::new();
        let mut review: Vec<String> = Vec::new();
        for entry in &snapshot.entries {
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
            // Links first, move last. A stop in between leaves links to an id no file has yet and the file under
            // its old name; running the same rename again finishes it. Moving first would leave links pointing
            // at nothing with no old name left to rename.
            for (entry, text) in &writes {
                self.store.save(lock, &entry.id, text)?;
            }
            self.store.rename(lock, &old, &new).map_err(|e| {
                EngineError::Invalid(format!(
                    "{e}; links already use {new}; run `stone rename {old} {new}` again"
                ))
            })?;
            self.sync_index(lock, |index| {
                let index = index_file::drop_line(index, &old);
                match &target.item {
                    Some(item) => index_file::put(&index, scheme, &new, item),
                    None => index,
                }
            })?;
        }
        Ok(Renamed {
            from,
            to,
            changed: true,
            rewritten,
            review,
        })
    }
}
