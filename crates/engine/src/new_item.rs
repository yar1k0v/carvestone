//! `stone new`: build the file text, then allocate the number and create the file under one lock.

use carvestone_core::{IdScheme, ItemId, ItemType, NumberedKind, sections, set_section};
use carvestone_store::store::item_from_document;
use carvestone_store::{Config, Document, FrontmatterError, StoreError};

use crate::create::{NewItem, Written, now, stamp_by, stamp_value};
use crate::fields::{SetCtx, SetValue, add_links, apply_set, apply_value};
use crate::index_file;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

const TYPE_NAMES: &str = "doc, plan, task, milestone, decision, reference, system, container, component";

/// Where the new file goes: a slug id is known now, a numbered id only under the lock.
enum Target {
    Slug(ItemId),
    Numbered(NumberedKind),
}

fn numbered_kind(ty: ItemType) -> Option<NumberedKind> {
    match ty {
        ItemType::Task => Some(NumberedKind::Task),
        ItemType::Milestone => Some(NumberedKind::Milestone),
        ItemType::Decision => Some(NumberedKind::Decision),
        _ => None,
    }
}

/// Section headings a new item of this type starts with.
fn template(ty: ItemType) -> &'static [&'static str] {
    match ty {
        ItemType::Task => &["Description", "Acceptance criteria"],
        ItemType::Decision => &["Context", "Decision", "Consequences"],
        ItemType::System | ItemType::Container | ItemType::Component => &["Responsibilities", "Interfaces", "Gotchas"],
        _ => &[],
    }
}

/// Names the front matter fields of a source file that the new item does not keep.
fn left_out_warning(req: &NewItem) -> Option<String> {
    if req.left_out.is_empty() {
        return None;
    }
    Some(format!(
        "left out front matter fields: {}; a file keeps only title, description, tags, links and stale_after, so set the others with --set",
        req.left_out.join(", ")
    ))
}

/// File-name slug of a title: lowercase ASCII letters and digits, any other character separates words.
pub(crate) fn ascii_slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn slug_of(ty: ItemType, req: &NewItem) -> Result<String, EngineError> {
    let Some(slug) = &req.slug else {
        let slug = ascii_slug(&req.title);
        if slug.is_empty() {
            return Err(EngineError::Invalid(
                "the title has no ASCII letters or digits; pass --slug".into(),
            ));
        }
        return Ok(slug);
    };
    let segments: Vec<&str> = slug.split('/').collect();
    let valid = (segments.len() == 1 || ty.folder() == "arch")
        && segments.iter().all(|s| {
            !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        });
    if valid {
        Ok(slug.clone())
    } else {
        Err(EngineError::Invalid(format!(
            "slug `{slug}` is not valid: use ASCII letters, digits, `.`, `_` and `-` (and `/` for arch items)"
        )))
    }
}

fn build_text(
    snapshot: &Snapshot,
    config: &Config,
    ty: ItemType,
    req: &NewItem,
    by: &str,
) -> Result<String, EngineError> {
    let mut text = format!("---\ntype: {}\n---\n", ty.as_str());
    for heading in template(ty) {
        text.push_str(&format!("\n## {heading}\n"));
    }
    let mut doc = Document::parse(&text)?;
    let ctx = SetCtx { snapshot, config, ty };
    apply_set(&ctx, &mut doc, "title", req.title.trim())?;
    let status = if ty == ItemType::Task {
        "todo".to_string()
    } else {
        config
            .statuses_for(ty)
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::Invalid(format!("no statuses are configured for a {}", ty.as_str())))?
    };
    apply_set(&ctx, &mut doc, "status", &status)?;
    let created = now();
    apply_set(&ctx, &mut doc, "created", &created)?;
    doc.set("generated", stamp_value(by, &created))?;
    for (key, value) in &req.set {
        apply_value(&ctx, &mut doc, key, value)?;
    }
    add_links(&snapshot.scheme, &mut doc, &req.links)?;
    if let Some(body) = &req.body {
        doc.body = if body.starts_with('\n') || body.starts_with("\r\n") {
            body.clone()
        } else {
            format!("\n{body}")
        };
    }
    for (name, section) in &req.sections {
        doc.body = set_section(&doc.body, name, section);
        if !doc.body.starts_with('\n') {
            doc.body.insert(0, '\n');
        }
    }
    Ok(doc.to_text())
}

impl NewItem {
    /// A request built from a whole markdown file: the title is the front matter `title`, else
    /// the first `# ` heading, else `stem`; `description`, `tags` and `links` come along (links as
    /// written); `stale_after` comes along too; the body is kept as written. A front matter field
    /// whose value does not reach the request (another key, or a value of the wrong shape) is
    /// named in `left_out`; `type` never is, because the command names the type. With no usable
    /// ASCII title the slug falls back to `stem`.
    pub fn from_markdown(type_name: &str, text: &str, stem: &str) -> Result<NewItem, EngineError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let (front, body) = match Document::parse(text) {
            Ok(doc) => {
                let body = doc.body.clone();
                (Some(doc), body)
            }
            Err(FrontmatterError::Missing) => (None, text.to_string()),
            Err(e) => return Err(e.into()),
        };
        let field = |key: &str| {
            front
                .as_ref()
                .and_then(|d| d.get_str(key))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        // The body's own scanner knows which `#` lines sit inside a code fence.
        let heading = sections(&body, 1)
            .into_iter()
            .find(|s| s.level == 1 && !s.title.is_empty())
            .map(|s| s.title);
        let own_title = field("title");
        let description = field("description");
        let stale_after = field("stale_after");
        let tags = front.as_ref().and_then(|d| d.get_list("tags"));
        let links = front.as_ref().and_then(|d| d.get_list("links"));
        let left_out = front
            .as_ref()
            .map(|d| {
                d.fields()
                    .keys()
                    .filter_map(|key| {
                        let name = match key.as_str() {
                            Some("type") => return None,
                            Some("title") if own_title.is_some() => return None,
                            Some("description") if description.is_some() => return None,
                            Some("stale_after") if stale_after.is_some() => return None,
                            Some("tags") if tags.is_some() => return None,
                            Some("links") if links.is_some() => return None,
                            Some(name) => name.to_string(),
                            // A key that is not text is named as the file wrote it.
                            None => serde_norway::to_string(key)
                                .map(|yaml| yaml.trim().to_string())
                                .unwrap_or_else(|_| "a key that is not text".to_string()),
                        };
                        Some(name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let title = own_title.or(heading).unwrap_or_else(|| stem.trim().to_string());
        let mut set = Vec::new();
        if let Some(description) = description {
            set.push(("description".to_string(), SetValue::Text(description)));
        }
        if let Some(stale_after) = stale_after {
            set.push(("stale_after".to_string(), SetValue::Text(stale_after)));
        }
        let tags = tags.unwrap_or_default();
        if !tags.is_empty() {
            set.push(("tags".to_string(), SetValue::List(tags)));
        }
        let slug = ascii_slug(&title)
            .is_empty()
            .then(|| ascii_slug(stem))
            .filter(|s| !s.is_empty());
        Ok(NewItem {
            type_name: type_name.to_string(),
            title,
            slug,
            set,
            sections: Vec::new(),
            body: Some(body),
            links: links.unwrap_or_default(),
            left_out,
        })
    }
}

/// The id and path a numbered item would get, with `<next>` where the number goes: a dry run must not take the lock the number needs.
fn next_label(scheme: &IdScheme, kind: NumberedKind) -> (String, String) {
    let sample = scheme.numbered_id(kind, 1);
    let with_next = |s: &str| {
        s.strip_suffix('1')
            .map(|stem| format!("{stem}<next>"))
            .unwrap_or_else(|| s.to_string())
    };
    let short = scheme.short(&sample).unwrap_or_else(|| sample.to_string());
    (with_next(&short), format!("stone/{}.md", with_next(sample.as_str())))
}

impl Repo {
    pub fn create(&self, req: &NewItem, dry_run: bool) -> Result<Written, EngineError> {
        let ty = ItemType::from_name(&req.type_name)
            .ok_or_else(|| EngineError::Invalid(format!("unknown type `{}`; one of {TYPE_NAMES}", req.type_name)))?;
        if req.title.trim().is_empty() {
            return Err(EngineError::Invalid("a title is required; pass --title".into()));
        }
        if req.body.is_some() && !template(ty).is_empty() {
            return Err(EngineError::Invalid(format!(
                "a whole file can only become a doc, plan or reference; a {} starts from its own sections",
                ty.as_str()
            )));
        }
        let target = match numbered_kind(ty) {
            Some(kind) => {
                if req.slug.is_some() {
                    return Err(EngineError::Invalid(
                        "--slug is for docs, plans, refs and arch items; numbered items get the next number".into(),
                    ));
                }
                Target::Numbered(kind)
            }
            None => Target::Slug(ItemId::from_path(&format!("{}/{}", ty.folder(), slug_of(ty, req)?))),
        };
        let by = stamp_by(self)?;
        // Read before the lock and used only to validate relations: it never decides an id or the text of the file.
        let snapshot = self.snapshot()?;
        let text = build_text(&snapshot, &self.store.config, ty, req, &by)?;
        let scheme = self.store.config.scheme();

        if dry_run {
            let (id, path) = match &target {
                Target::Slug(id) => {
                    if snapshot
                        .entries
                        .iter()
                        .any(|e| e.id.as_str().eq_ignore_ascii_case(id.as_str()))
                    {
                        return Err(StoreError::Exists(id.to_string()).into());
                    }
                    (id.to_string(), format!("stone/{id}.md"))
                }
                Target::Numbered(kind) => next_label(&scheme, *kind),
            };
            return Ok(Written {
                id,
                path,
                changed: true,
                text,
                warnings: left_out_warning(req).into_iter().collect(),
            });
        }

        // One lock for the whole allocate-then-create: a second writer waits here and then sees this file.
        let lock = self.store.lock()?;
        let (id, mut warnings): (ItemId, Vec<String>) = match &target {
            Target::Slug(id) => {
                // The same whole-id, any-case check as the dry run: the store only compares the last path segment.
                if self
                    .snapshot()?
                    .entries
                    .iter()
                    .any(|e| e.id.as_str().eq_ignore_ascii_case(id.as_str()))
                {
                    return Err(StoreError::Exists(id.to_string()).into());
                }
                (id.clone(), Vec::new())
            }
            Target::Numbered(kind) => {
                let allocation = self.store.next_number(&lock, *kind)?;
                (
                    scheme.numbered_id(*kind, allocation.number),
                    allocation.warning.into_iter().collect(),
                )
            }
        };
        self.store.create(&lock, &id, &text)?;
        if let Ok(doc) = Document::parse(&text) {
            let item = item_from_document(id.clone(), &doc);
            self.sync_index(&lock, |index| index_file::put(index, &scheme, &id, &item))?;
        }
        drop(lock);
        warnings.extend(left_out_warning(req));
        Ok(Written {
            id: scheme.short(&id).unwrap_or_else(|| id.to_string()),
            path: format!("stone/{id}.md"),
            changed: true,
            text,
            warnings,
        })
    }
}
