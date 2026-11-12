//! Changing items: `edit`, and the locked read-modify-write every writer shares.

use carvestone_core::{Actor, ItemType, Section, check_box, remove_section, section_text, sections, set_section};
use carvestone_store::store::item_from_document;
use carvestone_store::{Document, Entry, StoreError};

use crate::create::{Written, now, stamp_by, stamp_value, today};
use crate::fields::{SetCtx, SetValue, apply_unset, apply_value, check_done_sections, sync_closed};
use crate::index_file;
use crate::query::etag_of;
use crate::ready;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::stale::can_be_stale;
use serde_norway::Value;

/// What `stone edit` was asked to change. `section` is a name and its new text.
#[derive(Debug, Clone, Default)]
pub struct Edit {
    pub set: Vec<(String, SetValue)>,
    pub unset: Vec<String>,
    pub section: Option<(String, String)>,
    /// Replace the section although it holds sub-sections the new text does not carry.
    pub replace_nested: bool,
    /// A section to remove, with everything under it.
    pub remove_section: Option<String>,
    /// The 1-based checkboxes of `## Acceptance criteria` to tick.
    pub check_ac: Vec<usize>,
    /// One line to add under `## Notes`, dated today.
    pub note: Option<String>,
    /// Set a task to in-progress although the ready check finds gaps.
    pub force: bool,
    /// Append a `verified` stamp by this actor: the item was re-read and still holds.
    pub verify: Option<String>,
}

/// The item's type, or a pointer to `stone check` when the file has none that is known.
pub(crate) fn type_of(entry: &Entry) -> Result<ItemType, EngineError> {
    entry
        .item
        .as_ref()
        .and_then(|item| item.item_type())
        .ok_or_else(|| EngineError::Invalid(format!("{} has no known `type`; run `stone check`", entry.path)))
}

impl Repo {
    /// The one way an existing item is changed: lock, re-read, let `plan` change a copy of the
    /// document, save only if the text differs. A dry run takes no lock and never saves.
    pub(crate) fn update<F>(&self, input: &str, dry_run: bool, plan: F) -> Result<Written, EngineError>
    where
        F: FnOnce(&Snapshot, &Entry, &mut Document) -> Result<(), EngineError>,
    {
        self.update_if(input, dry_run, None, plan)
    }

    /// `update`, refusing to write when `if_match` is not the etag of the file as it is under the lock.
    pub(crate) fn update_if<F>(
        &self,
        input: &str,
        dry_run: bool,
        if_match: Option<&str>,
        plan: F,
    ) -> Result<Written, EngineError>
    where
        F: FnOnce(&Snapshot, &Entry, &mut Document) -> Result<(), EngineError>,
    {
        self.refuse_doc_file(input, false)?;
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        // Read after the lock: what is computed below is never based on an older copy of the file.
        let snapshot = self.snapshot()?;
        let (entry, anchor) = snapshot
            .resolve(input)
            .map_err(|err| self.refuse_doc_file(input, true).err().unwrap_or(err))?;
        if let Some(wanted) = if_match
            && wanted != etag_of(&entry.text)
        {
            return Err(EngineError::Conflict(format!(
                "{} changed on disk; reload",
                entry.id.as_str()
            )));
        }
        // The scan decodes lossily; saving that text would turn every bad byte into U+FFFD.
        if anchor.is_none() {
            let path = self.store.layout.item_path(&entry.id);
            let bytes = std::fs::read(&path).map_err(|source| {
                EngineError::from(StoreError::Io {
                    path: path.display().to_string(),
                    source,
                })
            })?;
            if std::str::from_utf8(&bytes).is_err() {
                return Err(EngineError::Invalid(format!(
                    "{} is not valid UTF-8; nothing was written",
                    entry.path
                )));
            }
        }
        if anchor.is_some() {
            return Err(EngineError::Invalid(format!(
                "`{input}` names a section; pass the item id and use --section to change a section"
            )));
        }
        let parsed = match &entry.doc {
            Ok(doc) => doc,
            Err(e) => {
                return Err(EngineError::Invalid(format!(
                    "{} has broken frontmatter ({e}); run `stone check`",
                    entry.path
                )));
            }
        };
        if let Some((line, why)) = parsed.subset_problems().into_iter().next() {
            return Err(EngineError::Invalid(format!(
                "{}:{line}: {why}; fix it by hand, nothing was written",
                entry.path
            )));
        }
        let mut doc = parsed.clone();
        plan(&snapshot, entry, &mut doc)?;
        let text = doc.to_text();
        let changed = text != entry.text;
        if changed && let Some(lock) = &lock {
            self.store.save(lock, &entry.id, &text)?;
            let item = item_from_document(entry.id.clone(), &doc);
            self.sync_index(lock, |index| index_file::put(index, &snapshot.scheme, &entry.id, &item))?;
        }
        Ok(Written {
            id: snapshot.label(entry),
            path: entry.path.clone(),
            changed,
            text,
            warnings: Vec::new(),
        })
    }
}

impl Repo {
    /// Fails with a plain sentence when `input` names a file in a doc folder, which stone reads
    /// and never writes. A name with `.md` counts first (a store id is written without it);
    /// one without it only counts after the store has no item of that name (`not_found`).
    pub(crate) fn refuse_doc_file(&self, input: &str, not_found: bool) -> Result<(), EngineError> {
        match self.doc_file_named(input) {
            Some(doc) if doc.has_ext != not_found => {
                Err(EngineError::Invalid(if self.store.config.is_doc_folder(&doc.folder) {
                    format!(
                        "{} is in {}/, a read-only doc folder: stone reads it for search and links and never writes there; edit the file by hand",
                        doc.file, doc.folder
                    )
                } else {
                    format!(
                        "{} is a project file: stone reads it for search and links and never writes it; edit the file by hand",
                        doc.file
                    )
                }))
            }
            _ => Ok(()),
        }
    }
}

/// The section a name points at: a title first, then an anchor, both ignoring ASCII case.
fn find_section<'a>(all: &'a [Section], name: &str) -> Option<&'a Section> {
    let name = name.trim();
    all.iter()
        .find(|s| s.title.eq_ignore_ascii_case(name))
        .or_else(|| all.iter().find(|s| s.anchor == name.to_lowercase()))
}

/// The title of the section `name` points at, with the titles of its sub-sections that `text`
/// does not carry, in document order. `None` when the section is missing or holds none.
fn dropped_nested(body: &str, name: &str, text: &str) -> Option<(String, Vec<String>)> {
    let all = sections(body, 1);
    let section = find_section(&all, name)?;
    let new = sections(text, 1);
    let dropped: Vec<String> = all
        .iter()
        .filter(|s| s.start_line > section.start_line && s.end_line <= section.end_line)
        .filter(|s| !new.iter().any(|n| n.title.eq_ignore_ascii_case(&s.title)))
        .map(|s| s.title.clone())
        .collect();
    (!dropped.is_empty()).then(|| (section.title.clone(), dropped))
}

/// The section and its heading line, when `text` starts with a heading that names the section
/// itself. Stone keeps the heading, so a heading in the text would be written twice.
fn repeated_heading(body: &str, name: &str, text: &str) -> Option<(String, String)> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) || !line[hashes..].starts_with(' ') {
        return None;
    }
    let title = line[hashes..].trim();
    let name = name.trim();
    let all = sections(body, 1);
    let own = find_section(&all, name).map_or(name, |s| s.title.as_str());
    (title.eq_ignore_ascii_case(name) || title.eq_ignore_ascii_case(own))
        .then(|| (own.to_string(), line.trim().to_string()))
}

/// `set_section`, except that text the section already holds leaves the body alone.
fn replace_section(body: &str, name: &str, text: &str) -> String {
    let flat = |t: &str| t.trim().replace("\r\n", "\n");
    let name = name.trim();
    let all = sections(body, 1);
    if let Some(section) = find_section(&all, name) {
        let held = section_text(body, 1, section)
            .split_once('\n')
            .map_or("", |(_, rest)| rest);
        if flat(held) == flat(text) {
            return body.to_string();
        }
    } else if name.eq_ignore_ascii_case("body") {
        return replace_preamble(body, text);
    }
    set_section(body, name, text)
}

/// Replaces the text before the first heading and keeps every section after it byte for byte.
/// Text the preamble already holds leaves the body alone.
fn replace_preamble(body: &str, text: &str) -> String {
    let eol = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let end = sections(body, 1).first().map_or(body.len(), |first| {
        body.split_inclusive('\n')
            .take(first.start_line - 1)
            .map(str::len)
            .sum()
    });
    let preamble = &body[..end];
    if preamble.trim().replace("\r\n", "\n") == text.trim().replace("\r\n", "\n") {
        return body.to_string();
    }
    // The blank lines between the front matter and the text stay as they are.
    let lead = &preamble[..preamble.len() - preamble.trim_start().len()];
    let lead = lead.rfind('\n').map_or("", |i| &lead[..=i]);
    let mut out = format!("{lead}{}{eol}", text.trim().replace("\r\n", "\n").replace('\n', eol));
    if end < body.len() {
        out.push_str(eol);
        out.push_str(&body[end..]);
    }
    out
}

/// Adds `- {day}: {text}` to the end of `## Notes`, or starts the section. Whitespace runs in
/// `text`, newlines included, become one space, so a comment can never start a heading. A line
/// the section already holds leaves the body alone.
fn add_note(body: &str, text: &str, day: &str) -> Result<String, EngineError> {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return Err(EngineError::Invalid(
            "the note is empty; write what has to change".into(),
        ));
    }
    let line = format!("- {day}: {flat}");
    let all = sections(body, 1);
    let held = all
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("notes"))
        .map_or("", |s| {
            section_text(body, 1, s).split_once('\n').map_or("", |(_, rest)| rest)
        })
        .trim();
    if held.lines().any(|l| l.trim_end() == line) {
        return Ok(body.to_string());
    }
    let next = if held.is_empty() {
        line
    } else {
        format!("{held}\n{line}")
    };
    Ok(set_section(body, "Notes", &next))
}

impl Repo {
    pub fn edit(&self, input: &str, req: &Edit, dry_run: bool) -> Result<Written, EngineError> {
        self.edit_if_match(input, req, dry_run, None)
    }

    /// `edit` that writes only when `if_match` is the etag of the file under the lock.
    pub fn edit_if_match(
        &self,
        input: &str,
        req: &Edit,
        dry_run: bool,
        if_match: Option<&str>,
    ) -> Result<Written, EngineError> {
        if req.set.is_empty()
            && req.unset.is_empty()
            && req.section.is_none()
            && req.remove_section.is_none()
            && req.check_ac.is_empty()
            && req.note.is_none()
            && req.verify.is_none()
        {
            return Err(EngineError::Invalid(
                "nothing to change; pass --set, --unset, --section, --remove-section, --check-ac or --verify".into(),
            ));
        }
        if req.section.is_some() && req.remove_section.is_some() {
            return Err(EngineError::Invalid(
                "pass either --section or --remove-section, not both".into(),
            ));
        }
        self.update_if(input, dry_run, if_match, |snapshot, entry, doc| {
            let ty = type_of(entry)?;
            let ctx = SetCtx {
                snapshot,
                config: &self.store.config,
                ty,
            };
            let (title, description) = (doc.get_str("title"), doc.get_str("description"));
            let body = doc.body.clone();
            let was = doc.get_str("status");
            for (key, value) in &req.set {
                apply_value(&ctx, doc, key, value)?;
            }
            for key in &req.unset {
                apply_unset(doc, key)?;
            }
            if let Some((name, text)) = &req.section {
                if let Some((section, line)) = repeated_heading(&doc.body, name, text) {
                    return Err(EngineError::Invalid(format!(
                        "the text for section `{section}` starts with its own heading `{line}`; Stone keeps the heading, so send the body only"
                    )));
                }
                if !req.replace_nested
                    && let Some((section, dropped)) = dropped_nested(&doc.body, name, text)
                {
                    let noun = if dropped.len() == 1 { "sub-section" } else { "sub-sections" };
                    return Err(EngineError::Invalid(format!(
                        "section `{section}` holds {} {noun} the new text does not carry ({}); edit each by name, or pass --replace-nested to replace them all",
                        dropped.len(),
                        dropped.join(", ")
                    )));
                }
                doc.body = replace_section(&doc.body, name, text);
            }
            if let Some(name) = &req.remove_section {
                doc.body = remove_section(&doc.body, name).ok_or_else(|| {
                    let id = snapshot.label(entry);
                    EngineError::Invalid(format!(
                        "{id} has no section `{}`; run `stone get {id}` to see its sections",
                        name.trim()
                    ))
                })?;
            }
            for &n in &req.check_ac {
                doc.body = check_box(&doc.body, "Acceptance criteria", n)?;
            }
            if let Some(text) = &req.note {
                doc.body = add_note(&doc.body, text, &today())?;
            }
            if let Some(by) = &req.verify {
                if !can_be_stale(Some(ty)) {
                    return Err(EngineError::Invalid(format!(
                        "{} is a {}; only docs, references and architecture items take a verified stamp",
                        snapshot.label(entry),
                        ty.as_str()
                    )));
                }
                if let Err(e) = by.parse::<Actor>() {
                    return Err(EngineError::Invalid(format!(
                        "cannot stamp as `{by}`: {e}; pass --by human:<id>, process:<id> or a producer name such as claude-code"
                    )));
                }
                // The old stamps stay as they are; a bare map becomes the first entry of a list.
                let mut stamps: Vec<Value> = doc
                    .get_stamps("verified")?
                    .iter()
                    .map(|s| stamp_value(&s.by, &s.at))
                    .collect();
                stamps.push(stamp_value(by, &now()));
                doc.set("verified", Value::Sequence(stamps))?;
            }
            if ty == ItemType::Task {
                sync_closed(&ctx, doc, was.as_deref())?;
                if !req.force
                    && doc.get_str("status").as_deref() == Some("in-progress")
                    && was.as_deref() != Some("in-progress")
                {
                    // The edited document is judged, so one edit may fill a gap and start the task.
                    let id = snapshot.label(entry);
                    let edited = item_from_document(entry.id.clone(), doc);
                    ready::refuse_unready(
                        self,
                        &id,
                        &edited,
                        &format!("stone edit {id} --set status=in-progress --force"),
                    )?;
                }
            }
            let is_done = ty == ItemType::Task && doc.get_str("status").as_deref() == Some("done");
            if is_done {
                check_done_sections(&self.store.config, &snapshot.label(entry), &doc.body, doc.body_line())?;
            }
            let touched =
                doc.get_str("title") != title || doc.get_str("description") != description || doc.body != body;
            if touched {
                doc.set("generated", stamp_value(&stamp_by(self)?, &now()))?;
            }
            Ok(())
        })
    }
}
