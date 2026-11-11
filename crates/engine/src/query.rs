//! Read side: `list` and `get`.

use std::collections::{HashMap, HashSet};

use carvestone_core::{Checkbox, IdScheme, Item, ItemType, Section, Trust, checkboxes, section_text, sections, trust};
use serde::Serialize;

use crate::arch_graph::{ComponentEdge, component_edges_in, is_arch};
use crate::changed_files::{ChangedFile, changed_in};
use crate::check_links::{CodeLink, CodeResolver};
use crate::git_cache::{history_ready, index_with_history};
use crate::refresh::refresh;
use crate::repo::{EngineError, Repo};
use crate::stale::{Stale, stale_in, staleness_in};
use crate::timeline::Date;

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub item_type: Option<String>,
    pub status: Option<String>,
    pub milestone: Option<String>,
    pub assignee: Option<String>,
    pub component: Option<String>,
    /// Only items that are stale (see `stale`).
    pub stale: bool,
    /// Fill `changed_files` on task rows; it reads the git cache, so plain lists leave it off.
    pub changed_files: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ListRow {
    pub id: String,
    pub short: Option<String>,
    pub path: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    /// The doc's sort position from front matter; left out when it writes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub order: Option<f64>,
    /// The doc's assignee from front matter; left out when it writes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub assignee: Option<String>,
    /// The doc's milestone from front matter; left out when it writes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub milestone: Option<String>,
    /// The plan a task belongs to, as its front matter writes it; left out when it names none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub plan: Option<String>,
    pub title: Option<String>,
    /// Hash of the file as it is on disk; a write passes it back as `if_match`.
    pub etag: String,
    /// Paths a task changed, when the list was asked for them; left out when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub changed_files: Vec<String>,
    /// What a task waits for, as its front matter writes it; left out when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub depends_on: Vec<String>,
    /// The day a task or plan was created, as its front matter writes it; left out for other
    /// items and when it writes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub created: Option<String>,
    /// Ticked acceptance criteria of a task; left out when it has no criteria.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub criteria_done: Option<u32>,
    /// All acceptance criteria of a task; left out when it has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub criteria_total: Option<u32>,
    /// Titles of the containers a task's components belong to, in component order; left out when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub services: Vec<String>,
}

/// The etag of an item file: the blake3 hash of its text, in hex.
pub fn etag_of(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ItemView {
    pub id: String,
    pub short: Option<String>,
    pub path: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub etag: String,
    pub trust: Trust,
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub sections: Vec<Section>,
    pub body: String,
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub text: String,
    /// Empty for items that name no library, and then left out of the JSON.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<LibraryRef>>", optional))]
    pub libraries: Vec<LibraryRef>,
    /// What the code says a system, container or component depends on; left out when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<ComponentEdge>>", optional))]
    pub code_dependencies: Vec<ComponentEdge>,
    pub backlinks: Vec<Backlink>,
    /// A task's files from its `Refs` commits and, in progress, its working changes; left out when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<ChangedFile>>", optional))]
    pub changed_files: Vec<ChangedFile>,
    /// A task's acceptance criteria, as its card counts them; left out when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Checkbox>>", optional))]
    pub criteria: Vec<Checkbox>,
    /// A task's `## Outcome` text without its heading; left out when there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub outcome: Option<String>,
    /// Why the item needs another look; left out when it is fresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub stale: Option<Stale>,
}

/// The checkboxes of a task's `## Acceptance criteria`; the card and the task view both count from here.
fn task_criteria(item: &Item) -> Vec<Checkbox> {
    sections(&item.body, item.body_line)
        .iter()
        .find(|s| s.anchor == "acceptance-criteria")
        .map(|s| checkboxes(&item.body, item.body_line, s))
        .unwrap_or_default()
}

/// The text of a task's `## Outcome` without its heading line; `None` when it is empty.
fn task_outcome(item: &Item) -> Option<String> {
    let found = sections(&item.body, item.body_line);
    let section = found.iter().find(|s| s.anchor == "outcome")?;
    let text = section_text(&item.body, item.body_line, section);
    let text = text.split_once('\n').map_or("", |(_heading, rest)| rest).trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A symbol or a code file, found by `get` when the id is no stone item.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeView {
    /// The qualified name or the file path.
    pub id: String,
    pub path: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub lines: [u32; 2],
    /// Docs and tasks whose `[[code:...]]` links point at it, or at a symbol in the file.
    pub backlinks: Vec<Backlink>,
    /// Never set: code has no etag. The field makes `etag` readable on the whole `GetResult` union.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub etag: Option<String>,
}

/// A library an item names: the name, and the version when written as `name@version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LibraryRef {
    pub library: String,
    pub version: Option<String>,
}

/// `mediatr@12` is library `mediatr`, version `12`; `serilog` has no version.
pub fn parse_library(spec: &str) -> LibraryRef {
    match spec.split_once('@') {
        Some((library, version)) => LibraryRef {
            library: library.to_string(),
            version: Some(version.to_string()),
        },
        None => LibraryRef {
            library: spec.to_string(),
            version: None,
        },
    }
}

/// The line `stone get` prints for an item's libraries, or nothing when it names none.
pub fn libraries_note(libraries: &[LibraryRef]) -> Option<String> {
    if libraries.is_empty() {
        return None;
    }
    let list: Vec<String> = libraries
        .iter()
        .map(|LibraryRef { library, version }| match version {
            Some(version) => format!("{library}@{version}"),
            None => library.clone(),
        })
        .collect();
    Some(format!(
        "Libraries (look up in Docs MCP with search_docs): {}",
        list.join(", ")
    ))
}

/// The libraries an item names: its `libraries` list, or a reference's single `library`.
fn libraries_of(item: &Item) -> Vec<LibraryRef> {
    let libraries: Vec<LibraryRef> = item
        .fields
        .libraries
        .iter()
        .map(|spec| parse_library(spec.as_str()))
        .collect();
    if libraries.is_empty()
        && let Some(library) = &item.fields.library
    {
        return vec![LibraryRef {
            library: library.clone(),
            version: item.fields.version.clone(),
        }];
    }
    libraries
}

/// A link that points at an item or one of its sections, from the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Backlink {
    pub id: String,
    pub anchor: Option<String>,
    pub path: String,
    pub line: u32,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SectionView {
    pub id: String,
    pub short: Option<String>,
    pub path: String,
    pub anchor: String,
    pub title: String,
    pub level: u8,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    /// The section's checkboxes, numbered as `check_ac` counts them.
    pub criteria: Vec<Checkbox>,
    /// Hash of the whole file, not of the section.
    pub etag: String,
    pub backlinks: Vec<Backlink>,
}

impl GetResult {
    /// The etag of the file behind the result; code symbols have none.
    pub fn etag(&self) -> Option<&str> {
        match self {
            GetResult::Item(view) => Some(&view.etag),
            GetResult::Section(view) => Some(&view.etag),
            GetResult::Code(_) => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum GetResult {
    /// Boxed: an item view is far larger than the other two.
    Item(Box<ItemView>),
    Section(SectionView),
    Code(CodeView),
}

const TYPE_NAMES: &str = "doc, plan, task, milestone, decision, reference, system, container, component";

/// The id text when `text` parses as an id, else the lowercased text.
fn canon(scheme: &IdScheme, text: &str) -> String {
    match scheme.parse(text) {
        Ok(r) => r.id.as_str().to_string(),
        Err(_) => text.to_lowercase(),
    }
}

/// Titles of the containers `components` sit in, in component order and without repeats. A
/// component is found by walking its id up until a container id matches.
fn services_of(scheme: &IdScheme, containers: &HashMap<String, String>, components: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for component in components {
        let mut id = canon(scheme, component);
        let title = loop {
            if let Some(title) = containers.get(&id) {
                break Some(title);
            }
            match id.rfind('/') {
                Some(at) => id.truncate(at),
                None => break None,
            }
        };
        if let Some(title) = title
            && !out.contains(title)
        {
            out.push(title.clone());
        }
    }
    out
}

impl Repo {
    pub fn list(&self, filter: &ListFilter) -> Result<Vec<ListRow>, EngineError> {
        let wanted_type = match &filter.item_type {
            Some(name) => Some(
                ItemType::from_name(name)
                    .ok_or_else(|| EngineError::Invalid(format!("unknown type `{name}`; one of {TYPE_NAMES}")))?,
            ),
            None => None,
        };
        let snapshot = self.snapshot()?;
        let scheme = &snapshot.scheme;
        let milestone = filter.milestone.as_deref().map(|m| canon(scheme, m));
        let component = filter.component.as_deref().map(|c| canon(scheme, c));

        let stale: Option<HashSet<String>> = if filter.stale {
            Some(
                stale_in(self, &snapshot, Date::today())?
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect(),
            )
        } else {
            None
        };
        let history = if filter.changed_files {
            index_with_history(self)?
        } else {
            None
        };

        let containers: HashMap<String, String> = snapshot
            .entries
            .iter()
            .filter_map(|entry| {
                let item = entry.item.as_ref()?;
                (item.item_type() == Some(ItemType::Container)).then(|| {
                    (
                        entry.id.as_str().to_string(),
                        item.fields.title.clone().unwrap_or_default(),
                    )
                })
            })
            .collect();

        let mut rows: Vec<(ItemType, u32, ListRow)> = Vec::new();
        for entry in &snapshot.entries {
            let Some(item) = &entry.item else { continue };
            let Some(ty) = item.item_type() else { continue };
            let f = &item.fields;
            if wanted_type.is_some_and(|w| w != ty) {
                continue;
            }
            if stale.as_ref().is_some_and(|ids| !ids.contains(entry.id.as_str())) {
                continue;
            }
            if filter.status.as_ref().is_some_and(|s| f.status.as_ref() != Some(s)) {
                continue;
            }
            if let Some(m) = &milestone
                && f.milestone.as_deref().map(|v| canon(scheme, v)).as_ref() != Some(m)
            {
                continue;
            }
            if filter
                .assignee
                .as_deref()
                .is_some_and(|a| f.assignee.as_deref() != Some(a))
            {
                continue;
            }
            if let Some(c) = &component
                && !f.components.iter().any(|v| canon(scheme, v) == *c)
            {
                continue;
            }
            let number = scheme.number(&entry.id).map_or(u32::MAX, |(_, n)| n);
            let changed_files = match (&history, ty) {
                (Some(index), ItemType::Task) => changed_in(self, &snapshot, entry, index.conn())?
                    .into_iter()
                    .map(|file| file.path)
                    .collect(),
                _ => Vec::new(),
            };
            let is_task = ty == ItemType::Task;
            let (criteria_done, criteria_total) = if is_task {
                Some(task_criteria(item))
                    .filter(|boxes| !boxes.is_empty())
                    .map_or((None, None), |boxes| {
                        let done = boxes.iter().filter(|b| b.checked).count();
                        (u32::try_from(done).ok(), u32::try_from(boxes.len()).ok())
                    })
            } else {
                (None, None)
            };
            let services = if is_task {
                services_of(scheme, &containers, &f.components)
            } else {
                Vec::new()
            };
            rows.push((
                ty,
                number,
                ListRow {
                    id: entry.id.as_str().to_string(),
                    short: scheme.short(&entry.id),
                    path: entry.path.clone(),
                    type_name: ty.as_str().to_string(),
                    status: f.status.clone(),
                    priority: f.priority.clone(),
                    order: f.order.as_deref().and_then(|text| text.trim().parse().ok()),
                    assignee: f.assignee.clone(),
                    milestone: f.milestone.clone(),
                    plan: f.plan.clone(),
                    title: f.title.clone(),
                    etag: etag_of(&entry.text),
                    changed_files,
                    depends_on: f.depends_on.clone(),
                    created: if is_task || ty == ItemType::Plan {
                        f.created.clone()
                    } else {
                        None
                    },
                    criteria_done,
                    criteria_total,
                    services,
                },
            ));
        }
        rows.sort_by(|a, b| (a.0, a.1, &a.2.path).cmp(&(b.0, b.1, &b.2.path)));
        Ok(rows.into_iter().map(|(_, _, row)| row).collect())
    }

    /// The etag of the item file `input` names, as it is on disk now.
    pub fn etag(&self, input: &str) -> Result<String, EngineError> {
        let snapshot = self.snapshot()?;
        let (entry, _) = snapshot.resolve(input)?;
        Ok(etag_of(&entry.text))
    }

    /// Brings the index up to date; false in a checkout that cannot hold `.stone/`, which still
    /// prints the item, just without backlinks.
    fn refreshed(&self) -> Result<bool, EngineError> {
        match refresh(self) {
            Ok(_) => Ok(true),
            Err(EngineError::Invalid(msg)) if msg.starts_with("cannot write .stone/") => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn get(&self, input: &str) -> Result<GetResult, EngineError> {
        let snapshot = self.snapshot()?;
        // A name written with `.md` is a doc-root file first; one without only when no store item
        // has the name, as `edit` reads it.
        let mut doc_file = self.doc_file_named(input);
        if doc_file.as_ref().is_some_and(|d| d.has_ext)
            && let Some(doc) = doc_file.take()
        {
            return self.get_doc_file(doc, self.refreshed()?);
        }
        let (entry, anchor) = match snapshot.resolve(input) {
            Ok(found) => found,
            Err(err) => {
                if let Some(doc) = doc_file {
                    return self.get_doc_file(doc, self.refreshed()?);
                }
                // A markdown file path or a wrongly prefixed id: say what to run instead. A markdown
                // file is no code file, so this comes before the code lookup, which would take it.
                let not_found = matches!(err, EngineError::NotFound { .. });
                let name = input.split_once('#').map_or(input, |(name, _)| name);
                let on_disk = self
                    .store
                    .layout
                    .root
                    .join(name.trim().replace('\\', "/").trim_start_matches('/'))
                    .is_file();
                if not_found
                    && name.ends_with(".md")
                    && let Some(hint) = snapshot.redirect(input, on_disk)
                {
                    return Err(EngineError::Redirect(hint));
                }
                if let Some(found) = self.get_code(input)? {
                    return Ok(found);
                }
                if not_found && let Some(hint) = snapshot.redirect(input, on_disk) {
                    return Err(EngineError::Redirect(hint));
                }
                return Err(err);
            }
        };
        let (Ok(doc), Some(item)) = (&entry.doc, &entry.item) else {
            let why = entry.doc.as_ref().err().map(|e| e.to_string()).unwrap_or_default();
            return Err(EngineError::Invalid(format!(
                "{} has broken frontmatter ({why}); run `stone check`",
                entry.path
            )));
        };
        let id = entry.id.as_str().to_string();
        let short = snapshot.scheme.short(&entry.id);
        let path = entry.path.clone();
        let indexed = self.refreshed()?;
        match anchor {
            None => {
                // Stamps are shown in their normalised shape, not as written.
                let mut fields = doc.json_fields();
                if let Some(slot) = fields.get_mut("verified") {
                    *slot = serde_json::to_value(&item.fields.verified).unwrap_or_default();
                }
                if let (Some(slot), Some(stamp)) = (fields.get_mut("generated"), &item.fields.generated) {
                    *slot = serde_json::to_value(stamp).unwrap_or_default();
                }
                let backlinks = if indexed {
                    self.backlinks(&id, None)?
                } else {
                    Vec::new()
                };
                let code_dependencies = if indexed && is_arch(item.item_type()) {
                    let index = self.index()?;
                    component_edges_in(&snapshot, index.conn())?
                        .into_iter()
                        .filter(|edge| edge.from == id)
                        .collect()
                } else {
                    Vec::new()
                };
                let changed_files = if indexed && item.item_type() == Some(ItemType::Task) {
                    let index = self.index()?;
                    if history_ready(self, index.conn())? {
                        changed_in(self, &snapshot, entry, index.conn())?
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                let stale = if indexed {
                    staleness_in(self, &snapshot, &id, Date::today())?
                } else {
                    None
                };
                let is_task = item.item_type() == Some(ItemType::Task);
                Ok(GetResult::Item(Box::new(ItemView {
                    backlinks,
                    code_dependencies,
                    changed_files,
                    criteria: if is_task { task_criteria(item) } else { Vec::new() },
                    outcome: if is_task { task_outcome(item) } else { None },
                    stale,
                    etag: etag_of(&entry.text),
                    id,
                    short,
                    path,
                    type_name: item.type_name.clone().unwrap_or_default(),
                    trust: trust(&item.fields.verified),
                    fields,
                    sections: sections(&item.body, item.body_line),
                    body: item.body.clone(),
                    text: entry.text.clone(),
                    libraries: libraries_of(item),
                })))
            }
            Some(anchor) => {
                let section = snapshot.find_section(entry, &anchor)?;
                let text = section_text(&item.body, item.body_line, &section).to_string();
                let criteria = checkboxes(&item.body, item.body_line, &section);
                let backlinks = if indexed {
                    self.backlinks(&id, Some(&section.anchor))?
                } else {
                    Vec::new()
                };
                Ok(GetResult::Section(SectionView {
                    backlinks,
                    id,
                    short,
                    path,
                    anchor: section.anchor,
                    title: section.title,
                    level: section.level,
                    start_line: section.start_line,
                    end_line: section.end_line,
                    text,
                    criteria,
                    etag: etag_of(&entry.text),
                }))
            }
        }
    }
}

impl Repo {
    /// A symbol or code file named by `input`, with the docs that link to it; `None` when `input`
    /// names neither (or the checkout cannot hold the index).
    fn get_code(&self, input: &str) -> Result<Option<GetResult>, EngineError> {
        let index = match crate::code::open(self) {
            Ok(index) => index,
            Err(EngineError::Invalid(msg)) if msg.starts_with("cannot write .stone/") => return Ok(None),
            Err(err) => return Err(err),
        };
        let conn = index.conn();
        let mut resolver = CodeResolver::new(&self.store.layout.root, conn);
        let (id, path, type_name, lines, in_file) = match resolver.resolve(input)? {
            CodeLink::Symbol(qname) => {
                let (path, start, end): (String, u32, u32) = conn.query_row(
                    "SELECT f.path, s.line_start, s.line_end FROM symbols s JOIN files f ON f.id = s.file_id \
                     WHERE s.qname = ?1 AND s.kind != 'file' ORDER BY f.path, s.line_start LIMIT 1",
                    [&qname],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                (qname, path, "symbol", [start, end], None)
            }
            CodeLink::File(path) => {
                let lines = resolver.lines_of(&path).unwrap_or(1);
                (path.clone(), path.clone(), "file", [1, lines], Some(path))
            }
            CodeLink::Range { path, a, b } => (path.clone(), path.clone(), "file", [a, b], Some(path)),
            _ => return Ok(None),
        };
        // Links are stored as written, so each distinct target is resolved to see what it names.
        let mut stmt = conn.prepare("SELECT DISTINCT dst_item FROM links WHERE kind = 'code' ORDER BY dst_item")?;
        let written: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
        let mut backlinks: Vec<Backlink> = Vec::new();
        for dst in written {
            let names_it = match resolver.resolve(dst.strip_prefix("code:").unwrap_or(&dst))? {
                CodeLink::Symbol(qname) if type_name == "symbol" => qname == id,
                CodeLink::Symbol(qname) => match &in_file {
                    Some(file) => conn
                        .prepare_cached(
                            "SELECT 1 FROM symbols s JOIN files f ON f.id = s.file_id \
                             WHERE s.qname = ?1 AND f.path = ?2 LIMIT 1",
                        )?
                        .exists([&qname, file])?,
                    None => false,
                },
                CodeLink::File(p) | CodeLink::Range { path: p, .. } => in_file.as_deref() == Some(p.as_str()),
                _ => false,
            };
            if names_it {
                backlinks.extend(self.backlinks(&dst, None)?);
            }
        }
        backlinks.sort_by(|a, b| (&a.id, &a.anchor, a.line).cmp(&(&b.id, &b.anchor, b.line)));
        backlinks.dedup();
        Ok(Some(GetResult::Code(CodeView {
            id,
            path,
            type_name: type_name.to_string(),
            lines,
            backlinks,
            etag: None,
        })))
    }

    /// Links into `id`, or into one of its sections when `anchor` is given,
    /// sorted by source id, anchor and line.
    pub(crate) fn backlinks(&self, id: &str, anchor: Option<&str>) -> Result<Vec<Backlink>, EngineError> {
        let index = self.index()?;
        let mut stmt = index.conn().prepare(
            "SELECT l.src_item, l.src_anchor, f.path, l.line, l.kind FROM links l \
             JOIN files f ON f.item_id = l.src_item \
             WHERE l.dst_item = ?1 AND (?2 IS NULL OR l.dst_anchor = ?2) \
             ORDER BY l.src_item, l.src_anchor, l.line",
        )?;
        let rows = stmt.query_map(rusqlite::params![id, anchor], |row| {
            let src_anchor: String = row.get(1)?;
            Ok(Backlink {
                id: row.get(0)?,
                anchor: (!src_anchor.is_empty()).then_some(src_anchor),
                path: row.get(2)?,
                line: row.get(3)?,
                kind: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
