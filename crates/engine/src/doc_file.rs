//! A file under a doc root as `get` shows it: its text and sections, read-only, with no fields.

use carvestone_core::{checkboxes, section_text, sections, trust};

use crate::query::{GetResult, ItemView, SectionView, etag_of};
use crate::repo::{EngineError, Repo};

/// A doc-root file that exists, named by what a person typed.
pub(crate) struct DocFile {
    /// Root-relative with `/` and `.md`.
    pub file: String,
    /// The doc root that holds it.
    pub folder: String,
    /// The input wrote `.md`; a store id is written without it.
    pub has_ext: bool,
    pub anchor: Option<String>,
}

impl Repo {
    /// The doc-root file `input` names (`docs/setup.md`, `./docs/setup#deploy`, an absolute path,
    /// `\` separators), if it is on disk.
    pub(crate) fn doc_file_named(&self, input: &str) -> Option<DocFile> {
        let root = self.store.layout.root.to_string_lossy().replace('\\', "/");
        let (name, anchor) = match input.split_once('#') {
            Some((name, anchor)) => (name, Some(anchor.to_string())),
            None => (input, None),
        };
        let flat = name.replace('\\', "/");
        let rel = flat.strip_prefix(&format!("{root}/")).unwrap_or(&flat);
        let rel = rel.trim_start_matches("./").trim_start_matches('/');
        let has_ext = rel.ends_with(".md");
        let file = if has_ext { rel.to_string() } else { format!("{rel}.md") };
        let folder = self.store.config.doc_root_of(&file)?;
        self.store.layout.root.join(&file).is_file().then_some(DocFile {
            file,
            folder,
            has_ext,
            anchor,
        })
    }

    /// The file as an item with no fields, or one of its sections.
    pub(crate) fn get_doc_file(&self, doc: DocFile, indexed: bool) -> Result<GetResult, EngineError> {
        let stat = self.store.stat_doc(&doc.file)?.ok_or_else(|| EngineError::NotFound {
            id: doc.file.clone(),
            suggestion: None,
        })?;
        let entry = self.store.read_entry(&stat)?;
        let Some(item) = &entry.item else {
            return Err(EngineError::NotFound {
                id: doc.file,
                suggestion: None,
            });
        };
        let id = entry.id.as_str().to_string();
        let found = sections(&item.body, item.body_line);
        match doc.anchor {
            None => Ok(GetResult::Item(Box::new(ItemView {
                backlinks: if indexed {
                    self.backlinks(&id, None)?
                } else {
                    Vec::new()
                },
                etag: etag_of(&entry.text),
                short: None,
                path: entry.path.clone(),
                type_name: "doc".to_string(),
                trust: trust(&item.fields.verified),
                fields: serde_json::Map::new(),
                sections: found,
                body: item.body.clone(),
                text: entry.text.clone(),
                libraries: Vec::new(),
                code_dependencies: Vec::new(),
                changed_files: Vec::new(),
                criteria: Vec::new(),
                outcome: None,
                stale: if indexed {
                    crate::stale::plain_staleness(self, &entry, crate::timeline::Date::today())?
                } else {
                    None
                },
                id,
            }))),
            Some(anchor) => {
                let wanted = anchor.to_lowercase();
                let Some(section) = found.iter().find(|s| s.anchor == wanted).cloned() else {
                    return Err(EngineError::NoSection {
                        id: entry.path.clone(),
                        anchor,
                        anchors: found.into_iter().map(|s| s.anchor).collect(),
                    });
                };
                Ok(GetResult::Section(SectionView {
                    backlinks: if indexed {
                        self.backlinks(&id, Some(&section.anchor))?
                    } else {
                        Vec::new()
                    },
                    text: section_text(&item.body, item.body_line, &section).to_string(),
                    criteria: checkboxes(&item.body, item.body_line, &section),
                    etag: etag_of(&entry.text),
                    short: None,
                    path: entry.path.clone(),
                    anchor: section.anchor,
                    title: section.title,
                    level: section.level,
                    start_line: section.start_line,
                    end_line: section.end_line,
                    id,
                }))
            }
        }
    }
}
