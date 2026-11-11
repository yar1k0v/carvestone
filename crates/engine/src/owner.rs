//! Code ownership: each tracked file under `code_roots` belongs to one arch element.

use std::collections::HashMap;

use carvestone_core::ItemType;
use carvestone_graph::lang_of;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::check::Problem;
use crate::code_index::{list_files, root_prefixes};
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// An arch item that claims code, with the globs it claims it by.
pub(crate) struct Element {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) globs: GlobSet,
    /// What the elements below it (by `parent`) claim: theirs, not this one's.
    inner: GlobSet,
}

impl Element {
    /// Whether the file is this element's own: under its globs and not claimed by an element
    /// below it, so a container inside its system's folder owns its part of it.
    pub(crate) fn owns(&self, path: &str) -> bool {
        self.globs.is_match(path) && !self.inner.is_match(path)
    }
}

/// The arch items with a `code` list. A glob that does not compile is reported and claims nothing.
pub(crate) fn arch_elements(snapshot: &Snapshot, out: &mut Vec<Problem>) -> Vec<Element> {
    let mut elements = Vec::new();
    // (id, parent id, patterns) of each element, to give each one what its descendants claim.
    let mut claims: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    for entry in &snapshot.entries {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            continue;
        };
        if !matches!(
            item.item_type(),
            Some(ItemType::System | ItemType::Container | ItemType::Component)
        ) || item.fields.code.is_empty()
        {
            continue;
        }
        let line = doc.key_line("code").unwrap_or(1);
        let mut builder = GlobSetBuilder::new();
        for pattern in &item.fields.code {
            match GlobBuilder::new(pattern.trim()).literal_separator(true).build() {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(e) => out.push(Problem::new(
                    &entry.path,
                    line,
                    "code-glob",
                    format!("{e}; fix the code list"),
                )),
            }
        }
        match builder.build() {
            Ok(globs) => {
                let parent = item
                    .fields
                    .parent
                    .as_deref()
                    .and_then(|p| snapshot.resolve(p).ok())
                    .map(|(parent, _)| parent.id.as_str().to_string());
                claims.push((entry.id.as_str().to_string(), parent, item.fields.code.clone()));
                elements.push(Element {
                    id: entry.id.as_str().to_string(),
                    label: snapshot.label(entry),
                    globs,
                    inner: GlobSet::empty(),
                });
            }
            Err(e) => out.push(Problem::new(
                &entry.path,
                line,
                "code-glob",
                format!("{e}; fix the code list"),
            )),
        }
    }
    let parents: HashMap<&str, &str> = claims
        .iter()
        .filter_map(|(id, parent, _)| Some((id.as_str(), parent.as_deref()?)))
        .collect();
    for element in &mut elements {
        let mut builder = GlobSetBuilder::new();
        for (id, _, patterns) in &claims {
            // Walk up from `id`; the step limit stops a `parent` cycle.
            let mut up = parents.get(id.as_str()).copied();
            let mut below = false;
            for _ in 0..claims.len() {
                match up {
                    Some(p) if p == element.id => {
                        below = true;
                        break;
                    }
                    Some(p) => up = parents.get(p).copied(),
                    None => break,
                }
            }
            if below {
                for pattern in patterns {
                    if let Ok(glob) = GlobBuilder::new(pattern.trim()).literal_separator(true).build() {
                        builder.add(glob);
                    }
                }
            }
        }
        element.inner = builder.build().unwrap_or_else(|_| GlobSet::empty());
    }
    elements
}

/// The first arch item with a `code` list, as (path, line of that key).
fn first_code_item(snapshot: &Snapshot) -> Option<(String, usize)> {
    snapshot.entries.iter().find_map(|entry| {
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            return None;
        };
        let is_arch = matches!(
            item.item_type(),
            Some(ItemType::System | ItemType::Container | ItemType::Component)
        );
        (is_arch && !item.fields.code.is_empty()).then(|| (entry.path.clone(), doc.key_line("code").unwrap_or(1)))
    })
}

/// Files git tracks under `roots`, root-relative, from each repo that holds some of them; `None`
/// when git cannot list one of the roots (git is missing or the root is in no repository).
pub(crate) fn tracked(repo: &Repo, roots: &[String]) -> Option<Vec<String>> {
    let listing = list_files(repo, false);
    if !listing.unlisted.is_empty() {
        return None;
    }
    let prefixes = root_prefixes(roots);
    let mut files: Vec<String> = listing
        .paths
        .into_iter()
        .filter(|path| {
            !path.starts_with("stone/")
                && !path.starts_with(".stone/")
                && prefixes.iter().any(|prefix| path.starts_with(prefix.as_str()))
        })
        .collect();
    files.sort();
    files.dedup();
    Some(files)
}

impl Repo {
    /// Problems with code ownership, unsorted. Skipped when there are no `code_roots`; when git cannot
    /// list the files it is reported once if any element declares `code`.
    pub fn ownership(&self, snapshot: &Snapshot) -> Result<Vec<Problem>, EngineError> {
        let mut problems = Vec::new();
        let elements = arch_elements(snapshot, &mut problems);
        let roots = &self.store.config.code_roots;
        if roots.is_empty() {
            return Ok(problems);
        }
        // A new project has no components yet, and a whole-repo root would flag every file.
        if root_prefixes(roots).iter().all(String::is_empty) && first_code_item(snapshot).is_none() {
            return Ok(problems);
        }
        let Some(files) = tracked(self, roots) else {
            if let Some((path, line)) = first_code_item(snapshot) {
                problems.push(Problem::new(
                    &path,
                    line,
                    "owner",
                    "the code files could not be listed (git is missing or this is not a repository), so ownership was not checked",
                ));
            }
            return Ok(problems);
        };
        // A whole-repo root holds READMEs and ignore files, which no component should own.
        let whole_repo = root_prefixes(roots).iter().all(String::is_empty);
        for file in files {
            if whole_repo && lang_of(&file).is_none() {
                continue;
            }
            let owners: Vec<&str> = elements
                .iter()
                .filter(|e| e.owns(&file))
                .map(|e| e.label.as_str())
                .collect();
            match owners.len() {
                1 => {}
                0 => problems.push(Problem::new(
                    &file,
                    1,
                    "owner",
                    "no arch element owns this file; add it to the code globs of one component in stone/arch/",
                )),
                _ => problems.push(Problem::new(
                    &file,
                    1,
                    "owner",
                    format!(
                        "owned by {}; narrow their code globs so exactly one matches",
                        owners.join(", ")
                    ),
                )),
            }
        }
        Ok(problems)
    }
}
