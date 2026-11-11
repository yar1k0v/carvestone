//! `stone export --okf`: a one-way copy of the store where every relation is a standard markdown
//! link, for tools that read only markdown links.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use carvestone_core::wiki_links;
use carvestone_store::Entry;
use carvestone_store::write::write_atomic;
use serde::Serialize;

use crate::check_links::{RELATION_KEYS, relation_values};
use crate::index_file::link_text;
use crate::rename::{dir_of, parts, relative};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Exported {
    pub folder: String,
    pub files: usize,
    /// Links that name no item and stay as written, as `<file>: <link>`.
    pub unresolved: Vec<String>,
}

/// The link from `entry`'s exported file to the target's, with the anchor when there is one.
fn link_to(entry: &Entry, target: &Entry, anchor: Option<&str>) -> String {
    let from = parts(&format!("{}.md", entry.id));
    let mut link = relative(dir_of(&from), &parts(&format!("{}.md", target.id)));
    if let Some(anchor) = anchor {
        link.push('#');
        link.push_str(anchor);
    }
    link
}

/// The `## Links` section of the item's relations, or `None` when it has none.
fn links_section(snapshot: &Snapshot, entry: &Entry, eol: &str, unresolved: &mut Vec<String>) -> Option<String> {
    let item = entry.item.as_ref()?;
    let mut out = String::new();
    for key in RELATION_KEYS {
        let values: Vec<&str> = relation_values(&item.fields, key)
            .into_iter()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .collect();
        if values.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(&format!("## Links{eol}"));
        }
        out.push_str(&format!("{eol}### {key}{eol}{eol}"));
        for value in values {
            match snapshot.resolve(value) {
                Ok((target, anchor)) => {
                    let text = match &target.item {
                        Some(found) => link_text(&snapshot.scheme, &target.id, found),
                        None => target.id.to_string(),
                    };
                    out.push_str(&format!(
                        "- [{text}]({}){eol}",
                        link_to(entry, target, anchor.as_deref())
                    ));
                }
                Err(_) => {
                    unresolved.push(format!("{}: {value}", entry.path));
                    out.push_str(&format!("- {value}{eol}"));
                }
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The text of the exported file: body wikilinks as markdown links, then the `## Links` section.
/// A file stone cannot read as an item comes out as it is.
pub(crate) fn okf_text(snapshot: &Snapshot, entry: &Entry, unresolved: &mut Vec<String>) -> String {
    let (Some(item), Ok(doc)) = (&entry.item, &entry.doc) else {
        return entry.text.clone();
    };
    let eol = doc.eol();
    let mut body = item.body.clone();
    for hit in wiki_links(&item.body, item.body_line).into_iter().rev() {
        let replacement = if let Some(code) = hit.target.strip_prefix("code:") {
            // Code is outside the bundle, so there is nothing to link to.
            format!("`{}`", code.trim())
        } else {
            match snapshot.resolve(&hit.target) {
                Ok((target, anchor)) => {
                    format!("[{}]({})", hit.target, link_to(entry, target, anchor.as_deref()))
                }
                Err(_) => {
                    unresolved.push(format!("{}: [[{}]]", entry.path, hit.target));
                    continue;
                }
            }
        };
        body.replace_range(hit.start..hit.end, &replacement);
    }

    let mut out = doc.clone();
    out.body = body;
    if let Some(section) = links_section(snapshot, entry, eol, unresolved) {
        // The closing `---` of a file can lack its line ending; the heading needs its own line.
        if !out.to_text().ends_with('\n') {
            out.body.push_str(eol);
        }
        if !out.body.is_empty() {
            out.body.push_str(eol);
        }
        out.body.push_str(&section);
    }
    out.to_text()
}

/// The path with every existing part resolved by the file system, so letter case, links and `..`
/// follow the file system. A part that does not exist yet is kept as written.
fn real_path(path: &Path) -> PathBuf {
    let mut real = PathBuf::new();
    let mut missing: Vec<&OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => real.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if missing.pop().is_none() {
                    real.pop();
                }
            }
            Component::Normal(name) => {
                if missing.is_empty()
                    && let Ok(found) = fs::canonicalize(real.join(name))
                {
                    real = found;
                } else {
                    missing.push(name);
                }
            }
        }
    }
    real.extend(missing);
    real
}

/// Whether the file system ignores letter case, asked with a folder that exists: the same path
/// with its last part in the other case still exists.
fn ignores_case(existing: &Path) -> bool {
    let Some(name) = existing.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let flipped: String = name
        .chars()
        .flat_map(|c| {
            if c.is_lowercase() {
                c.to_uppercase().collect::<Vec<_>>()
            } else {
                c.to_lowercase().collect()
            }
        })
        .collect();
    flipped != name && existing.with_file_name(flipped).exists()
}

/// Whether the first part of `target` under `root` reads `.stone` in any letter case.
fn starts_with_derived_dir(target: &Path, root: &Path) -> bool {
    let Ok(rest) = target.strip_prefix(root) else {
        return false;
    };
    matches!(
        rest.components().next(),
        Some(Component::Normal(name)) if name.to_string_lossy().to_lowercase() == ".stone"
    )
}

impl Repo {
    /// Writes the copy into `folder`, which must be new or empty and outside the store. Nothing in
    /// the store is written, so no lock is taken.
    pub fn export_okf(&self, folder: &Path) -> Result<Exported, EngineError> {
        let layout = &self.store.layout;
        let absolute = std::path::absolute(folder).map_err(|source| carvestone_store::StoreError::Io {
            path: folder.display().to_string(),
            source,
        })?;
        let target = real_path(&absolute);
        let data_dir = real_path(&layout.data_dir);
        let root = real_path(&layout.root);
        let derived = real_path(&layout.root.join(".stone"));
        // A missing `.stone` leaves any letter case of it as written, so the data folder, which
        // exists, tells whether this file system treats the cases as one name.
        let other_case = !derived.exists() && ignores_case(&data_dir) && starts_with_derived_dir(&target, &root);
        if target.starts_with(&data_dir) || target.starts_with(&derived) || other_case {
            return Err(EngineError::Invalid(format!(
                "{} is inside the store; export into a folder outside stone/ and .stone/",
                folder.display()
            )));
        }
        if absolute.exists() {
            let empty = fs::read_dir(&absolute).is_ok_and(|mut dir| dir.next().is_none());
            if !empty {
                return Err(EngineError::Invalid(format!(
                    "{} is not empty; export into a new or empty folder",
                    folder.display()
                )));
            }
        }

        let snapshot = self.snapshot()?;
        let mut unresolved = Vec::new();
        let mut files = 0;
        for entry in &snapshot.entries {
            let text = okf_text(&snapshot, entry, &mut unresolved);
            write_atomic(&absolute.join(format!("{}.md", entry.id)), &text)?;
            files += 1;
        }
        for (path, text) in self.store.scan_reserved()? {
            let rel = path.strip_prefix("stone/").unwrap_or(&path);
            write_atomic(&absolute.join(rel), &text)?;
            files += 1;
        }
        unresolved.sort();
        Ok(Exported {
            folder: folder.display().to_string(),
            files,
            unresolved,
        })
    }
}
