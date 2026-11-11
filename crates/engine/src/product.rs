//! The product as its owner reads it: the overview, the features and what each feature connects to.
//!
//! A feature is a doc of the `feature` family, or a doc whose kind says so. The overview and the
//! catalog are found by name (`product-overview`, `feature-index`) or by kind; a project without
//! them has an empty product, not an error.
//!
//! A task belongs to a feature when one links the other. Failing that, to every feature that
//! shares a link, a tag or a code path with it, marked inferred. A link or tag most features
//! share tells nothing about any of them, so it does not count.

use std::collections::{BTreeMap, HashMap, HashSet};

use carvestone_core::{Item, ItemType, doc_facts, lines};
use carvestone_graph::lang_of;
use carvestone_store::{Config, Entry};
use serde::Serialize;

use crate::code_index::root_prefixes;
use crate::docs_list::{family, is_template, kind_of, summary_of, updated_days};
use crate::links::{LinkContext, links_of};
use crate::owner::arch_elements;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::stale::stale_in;
use crate::timeline::Date;
use crate::work_links::{TaskCommit, WorkLinks, clean_path, tasks};

const OVERVIEW_NAME: &str = "product-overview";
const CATALOG_NAME: &str = "feature-index";
const OVERVIEW_KINDS: [&str; 2] = ["product-overview", "product"];
const CATALOG_KINDS: [&str; 2] = ["feature-index", "feature-catalog"];
const FEATURE_FAMILY: &str = "feature";
/// A link target or tag on more features than this says nothing about one of them.
const MAX_SHARERS: usize = 3;
/// A folder must be this many levels deep before two files in it count as one area of the code.
const MIN_AREA_DEPTH: usize = 3;
/// Where a long `Status:` line stops being a status and starts being a story.
const STATUS_ENDS: [&str; 6] = [" (", ";", " — ", " - ", ". ", " · "];
const STATUS_CHARS: usize = 60;
/// Commits listed on a feature page.
const MAX_COMMITS: usize = 100;

/// The doc that says what the product is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProductOverview {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub summary: Option<String>,
}

/// One feature of the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Feature {
    pub id: String,
    pub title: String,
    /// The first clause of the page's `Status:` line, else the doc's `status`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,
    /// The names on the page's `Platforms:` line, without their notes in brackets.
    pub platforms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub summary: Option<String>,
    /// `YYYY-MM-DD` of the page's last change.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub updated: Option<String>,
    /// Tasks linked to the feature that are not finished.
    pub open_tasks: u32,
    /// Tasks linked to the feature that are done.
    pub done_tasks: u32,
    /// Unfinished tasks only inferred to belong here. One can count under several features.
    pub inferred_open_tasks: u32,
    /// Titles of the containers and systems the feature runs on, linked ones first.
    pub services: Vec<String>,
    /// The feature's doc is out of date: a `stale_after` date has passed, or code it names changed.
    pub stale: bool,
}

/// `GET /api/product`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Product {
    pub overview: Option<ProductOverview>,
    pub features: Vec<Feature>,
}

/// A doc or decision a feature links, or that links it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeatureDoc {
    pub id: String,
    pub title: String,
    /// `doc` or `decision`.
    #[serde(rename = "type")]
    #[cfg_attr(feature = "ts", ts(rename = "type"))]
    pub item_type: String,
}

/// A task of a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeatureTask {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub status: Option<String>,
    /// Nothing links the two; `via` says what they share.
    pub inferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub via: Option<String>,
    /// `YYYY-MM-DD` the task was closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub closed: Option<String>,
    /// `closed` is read from commits or the file, not recorded on the task.
    pub closed_estimated: bool,
}

/// A part of the architecture a feature runs on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeaturePart {
    pub id: String,
    pub title: String,
    /// `system`, `container` or `component`.
    pub kind: String,
    /// Not linked: it owns code the feature page or its tasks name.
    pub inferred: bool,
}

/// A commit of one of a feature's tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeatureCommit {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub commit: TaskCommit,
    /// The task that ties it to the feature.
    pub task: String,
    /// That task is only inferred to belong to the feature.
    pub inferred: bool,
}

/// A feature a task belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TaskFeature {
    pub id: String,
    pub title: String,
    /// What the two share, when nothing links them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub via: Option<String>,
}

/// What a feature connects to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeatureLinks {
    pub docs: Vec<FeatureDoc>,
    pub tasks: Vec<FeatureTask>,
    pub parts: Vec<FeaturePart>,
    /// The commits of its tasks, newest first.
    pub commits: Vec<FeatureCommit>,
}

/// `GET /api/features/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeatureDetail {
    #[serde(flatten)]
    pub feature: Feature,
    pub links: FeatureLinks,
}

type Doc<'a> = (&'a Entry, &'a Item);

/// A task's place in a feature.
struct Member<'a> {
    task: Doc<'a>,
    /// What the two share; `None` when one links the other.
    via: Option<String>,
}

/// The name of a doc without its folder: `docs/feature-x` is `feature-x`.
fn name_of(id: &str) -> &str {
    id.strip_prefix("docs/").unwrap_or(id)
}

fn is_overview(id: &str, kind: Option<&str>) -> bool {
    name_of(id) == OVERVIEW_NAME || kind.is_some_and(|k| OVERVIEW_KINDS.contains(&k))
}

fn is_catalog(id: &str, kind: Option<&str>) -> bool {
    name_of(id) == CATALOG_NAME || kind.is_some_and(|k| CATALOG_KINDS.contains(&k))
}

/// True for a doc that describes one feature.
fn is_feature(id: &str, title: &str, kind: Option<&str>) -> bool {
    let named = family(id).eq_ignore_ascii_case(FEATURE_FAMILY) || kind == Some(FEATURE_FAMILY);
    named && !is_catalog(id, kind) && !is_overview(id, kind) && !is_template(id, title)
}

fn title_of(snapshot: &Snapshot, (entry, item): Doc) -> String {
    item.fields.title.clone().unwrap_or_else(|| snapshot.label(entry))
}

/// The word a feature goes by in a tag: `docs/feature-fuel` is `fuel`, `docs/feature/fuel` too.
fn slug_of(id: &str) -> &str {
    let name = name_of(id);
    name.rsplit('/')
        .next()
        .map_or(name, |last| last.strip_prefix("feature-").unwrap_or(last))
}

/// The first clause of a status line, short enough for a table cell.
fn short_status(text: &str) -> String {
    let end = STATUS_ENDS.iter().filter_map(|mark| text.find(mark)).min();
    let head = text[..end.unwrap_or(text.len())].trim().trim_end_matches(['.', ',']);
    if head.chars().count() <= STATUS_CHARS {
        return head.to_string();
    }
    let cut: String = head.chars().take(STATUS_CHARS).collect();
    let end = cut.rfind(' ').unwrap_or(cut.len());
    format!("{}…", cut[..end].trim_end_matches([',', ';', ':']))
}

/// The names on a platforms line. It ends at the first full stop or dash outside brackets, since a
/// page may run on into a sentence; `·` separates names, and notes in brackets go.
fn platform_names(text: &str) -> Vec<String> {
    let mut names = vec![String::new()];
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '·' => names.push(String::new()),
            '—' => break,
            '.' if chars.peek().is_none_or(|next| next.is_whitespace()) => break,
            _ => {
                if let Some(name) = names.last_mut() {
                    name.push(c);
                }
            }
        }
    }
    names
        .iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// `(status, platforms)` of a feature page.
fn facts_of(item: &Item) -> (Option<String>, Vec<String>) {
    let facts = doc_facts(&item.body);
    let value = |label: &str| {
        facts
            .iter()
            .find(|fact| fact.label == label)
            .map(|fact| fact.value.as_str())
    };
    let status = value("status")
        .map(short_status)
        .filter(|status| !status.is_empty())
        .or_else(|| item.fields.status.clone());
    // A long status line can run straight into `— Platforms: ...` with no ` · ` before it.
    let platforms = value("platforms").or_else(|| Some(value("status")?.split_once("Platforms:")?.1));
    (status, platforms.map(platform_names).unwrap_or_default())
}

/// Code files a page names in its text, written from a code root: in a code span, in a
/// `[code: path]` note or bare. Fenced blocks are examples, not references.
fn named_paths(item: &Item, roots: &[String]) -> Vec<String> {
    let prefixes = root_prefixes(roots);
    let cuts = |c: char| c.is_whitespace() || matches!(c, '`' | '[' | ']' | '(' | ')' | ',' | ';' | '"' | '\'');
    let mut out = Vec::new();
    for (_, text, in_fence) in lines(&item.body, item.body_line) {
        if in_fence {
            continue;
        }
        for word in text.split(cuts) {
            let word = word.trim_end_matches(['.', ':']);
            let path = word.split([':', '#']).next().unwrap_or(word);
            let is_file = path.rsplit('/').next().is_some_and(|name| name.contains('.'));
            // An empty prefix matches every word, so it also needs a folder and a code file
            // extension, which keeps words such as `e.g` and `Node.js` out.
            let under_root = prefixes.iter().any(|prefix| {
                if prefix.is_empty() {
                    path.contains('/') && lang_of(path).is_some()
                } else {
                    path.starts_with(prefix.as_str())
                }
            });
            if is_file && under_root {
                out.push(path.to_string());
            }
        }
    }
    out
}

/// True when two paths are one file, or two files in one folder deep enough to be one area.
fn same_area(a: &str, b: &str) -> bool {
    let folder = |path: &str| path.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_string());
    a == b || (folder(a) == folder(b) && a.matches('/').count() >= MIN_AREA_DEPTH)
}

/// The product's docs and tasks and how they connect, read once for a page.
struct Catalog<'a> {
    snapshot: &'a Snapshot,
    overview: Option<Doc<'a>>,
    catalog: Option<Doc<'a>>,
    features: Vec<Doc<'a>>,
    /// Every link target by item id, in file order, without anchors.
    links: HashMap<&'a str, Vec<String>>,
    /// The code files each feature names, by item id.
    paths: HashMap<&'a str, Vec<String>>,
}

impl<'a> Catalog<'a> {
    fn read(repo: &Repo, snapshot: &'a Snapshot) -> Catalog<'a> {
        let context = LinkContext::new(&repo.store.config);
        let mut catalog = Catalog {
            snapshot,
            overview: None,
            catalog: None,
            features: Vec::new(),
            links: HashMap::new(),
            paths: HashMap::new(),
        };
        for entry in &snapshot.entries {
            let Some(item) = entry.item.as_ref() else {
                continue;
            };
            let id = entry.id.as_str();
            let targets = links_of(&context, entry, &[]).into_iter().map(|row| row.dst_item);
            catalog.links.insert(id, targets.collect());
            if item.item_type() != Some(ItemType::Doc) {
                continue;
            }
            let kind = kind_of(item);
            let kind = kind.as_deref();
            let title = title_of(snapshot, (entry, item));
            if is_overview(id, kind) {
                catalog.overview.get_or_insert((entry, item));
            } else if is_catalog(id, kind) {
                catalog.catalog.get_or_insert((entry, item));
            } else if is_feature(id, &title, kind) {
                catalog.features.push((entry, item));
                let declared = item.fields.paths.iter().chain(&item.fields.code);
                let mut paths: Vec<String> = declared.map(|p| clean_path(p).to_string()).collect();
                paths.extend(named_paths(item, &repo.store.config.code_roots));
                catalog.paths.insert(id, paths);
            }
        }
        catalog.sort_features();
        catalog
    }

    /// The catalog's own order when it lists a feature, then by title.
    fn sort_features(&mut self) {
        let listed: Vec<String> = self
            .catalog
            .map(|(entry, _)| self.links_from(entry).to_vec())
            .unwrap_or_default();
        let place = |id: &str| listed.iter().position(|target| target == id).unwrap_or(usize::MAX);
        let snapshot = self.snapshot;
        self.features.sort_by_cached_key(|doc| {
            let id = doc.0.id.as_str();
            (place(id), title_of(snapshot, *doc))
        });
    }

    fn links_from(&self, entry: &Entry) -> &[String] {
        self.links.get(entry.id.as_str()).map_or(&[], Vec::as_slice)
    }

    fn linked(&self, from: &Entry, to: &Entry) -> bool {
        self.links_from(from).iter().any(|target| target == to.id.as_str())
    }

    /// What says a task with no link to the feature still belongs to it.
    fn evidence(
        &self,
        task: Doc<'a>,
        feature: Doc<'a>,
        shared_links: &HashMap<&str, usize>,
        shared_tags: &HashMap<&str, usize>,
    ) -> Option<String> {
        let few = |count: Option<&usize>| count.is_some_and(|n| *n <= MAX_SHARERS);
        let targets = self.links_from(feature.0);
        let link = self
            .links_from(task.0)
            .iter()
            .find(|target| few(shared_links.get(target.as_str())) && targets.contains(*target));
        if let Some(link) = link {
            return Some(format!("link {link}"));
        }
        let slug = slug_of(feature.0.id.as_str());
        let tag = task.1.fields.tags.iter().find(|tag| {
            tag.eq_ignore_ascii_case(slug)
                || (feature.1.fields.tags.contains(*tag) && few(shared_tags.get(tag.as_str())))
        });
        if let Some(tag) = tag {
            return Some(format!("tag {tag}"));
        }
        let named = self.paths.get(feature.0.id.as_str())?;
        task.1
            .fields
            .paths
            .iter()
            .map(|path| clean_path(path))
            .find(|path| named.iter().any(|other| same_area(path, other)))
            .map(|path| format!("path {path}"))
    }

    /// The tasks of every feature, by item id.
    fn members(&self) -> HashMap<&'a str, Vec<Member<'a>>> {
        let mut shared_links: HashMap<&str, usize> = HashMap::new();
        let mut shared_tags: HashMap<&str, usize> = HashMap::new();
        for (entry, item) in &self.features {
            let targets: HashSet<&str> = self.links_from(entry).iter().map(String::as_str).collect();
            for target in targets {
                *shared_links.entry(target).or_default() += 1;
            }
            for tag in &item.fields.tags {
                *shared_tags.entry(tag.as_str()).or_default() += 1;
            }
        }
        let mut out: HashMap<&'a str, Vec<Member<'a>>> = HashMap::new();
        for task in tasks(self.snapshot) {
            let named: Vec<&Doc<'a>> = self
                .features
                .iter()
                .filter(|(entry, _)| self.linked(task.0, entry) || self.linked(entry, task.0))
                .collect();
            for feature in &named {
                out.entry(feature.0.id.as_str())
                    .or_default()
                    .push(Member { task, via: None });
            }
            if !named.is_empty() {
                continue;
            }
            for feature in &self.features {
                if let Some(via) = self.evidence(task, *feature, &shared_links, &shared_tags) {
                    out.entry(feature.0.id.as_str())
                        .or_default()
                        .push(Member { task, via: Some(via) });
                }
            }
        }
        out
    }

    fn row(
        &self,
        config: &Config,
        doc: Doc<'a>,
        updated: &BTreeMap<String, String>,
        members: &[Member<'a>],
        stale: &HashSet<String>,
    ) -> Feature {
        let (entry, item) = doc;
        let mut services: Vec<String> = Vec::new();
        for part in self.parts_of(doc, members) {
            if matches!(part.kind.as_str(), "container" | "system") && !services.contains(&part.title) {
                services.push(part.title);
            }
        }
        let (status, platforms) = facts_of(item);
        let status_of = |member: &Member<'a>| member.task.1.fields.status.clone().unwrap_or_default();
        let open = |member: &&Member<'a>| !config.task_finished(&status_of(member));
        let linked = || members.iter().filter(|member| member.via.is_none());
        let inferred = members.iter().filter(|member| member.via.is_some());
        Feature {
            id: self.snapshot.label(entry),
            title: title_of(self.snapshot, doc),
            status,
            platforms,
            summary: summary_of(item),
            updated: updated.get(&entry.path).cloned(),
            open_tasks: linked().filter(open).count() as u32,
            done_tasks: linked().filter(|member| status_of(member) == "done").count() as u32,
            inferred_open_tasks: inferred.filter(open).count() as u32,
            services,
            stale: stale.contains(entry.id.as_str()),
        }
    }

    /// The docs and decisions on either end of a link with the feature; the catalog and the overview
    /// link everything, so they tell nothing.
    fn docs_of(&self, feature: &Entry) -> Vec<FeatureDoc> {
        let skipped: Vec<&str> = [self.catalog, self.overview]
            .iter()
            .flatten()
            .map(|(entry, _)| entry.id.as_str())
            .chain([feature.id.as_str()])
            .collect();
        let mut out: Vec<FeatureDoc> = Vec::new();
        for entry in &self.snapshot.entries {
            let Some(item) = entry.item.as_ref() else {
                continue;
            };
            let name = match item.item_type() {
                Some(ItemType::Doc) => "doc",
                Some(ItemType::Decision) => "decision",
                _ => continue,
            };
            if skipped.contains(&entry.id.as_str()) || !(self.linked(feature, entry) || self.linked(entry, feature)) {
                continue;
            }
            out.push(FeatureDoc {
                id: self.snapshot.label(entry),
                title: title_of(self.snapshot, (entry, item)),
                item_type: name.to_string(),
            });
        }
        out.sort_by(|a, b| a.title.cmp(&b.title));
        out
    }

    /// The arch items the feature links, then the ones that own code it or its linked tasks name.
    fn parts_of(&self, feature: Doc<'a>, members: &[Member<'a>]) -> Vec<FeaturePart> {
        let is_part = |entry: &Entry| {
            entry
                .item
                .as_ref()
                .and_then(Item::item_type)
                .is_some_and(|ty| matches!(ty, ItemType::System | ItemType::Container | ItemType::Component))
        };
        let title = |entry: &'a Entry| match &entry.item {
            Some(item) => title_of(self.snapshot, (entry, item)),
            None => self.snapshot.label(entry),
        };
        let kind = |entry: &Entry| {
            let ty = entry.item.as_ref().and_then(Item::item_type);
            ty.map_or(String::new(), |ty| ty.as_str().to_string())
        };
        let mut out: Vec<FeaturePart> = self
            .snapshot
            .entries
            .iter()
            .filter(|entry| is_part(entry) && self.linked(feature.0, entry))
            .map(|entry| FeaturePart {
                id: self.snapshot.label(entry),
                title: title(entry),
                kind: kind(entry),
                inferred: false,
            })
            .collect();
        let task_paths = members.iter().filter(|member| member.via.is_none()).flat_map(|member| {
            member
                .task
                .1
                .fields
                .paths
                .iter()
                .map(|path| clean_path(path).to_string())
        });
        let named = self.paths.get(feature.0.id.as_str()).cloned().unwrap_or_default();
        let paths: Vec<String> = named.into_iter().chain(task_paths).collect();
        let mut problems = Vec::new();
        for element in arch_elements(self.snapshot, &mut problems) {
            let Some(entry) = self.snapshot.entries.iter().find(|e| e.id.as_str() == element.id) else {
                continue;
            };
            let id = self.snapshot.label(entry);
            if !out.iter().any(|part| part.id == id) && paths.iter().any(|path| element.owns(path)) {
                out.push(FeaturePart {
                    id,
                    title: title(entry),
                    kind: kind(entry),
                    inferred: true,
                });
            }
        }
        out
    }
}

/// The ids of the docs whose text is out of date today.
fn stale_ids(repo: &Repo, snapshot: &Snapshot) -> Result<HashSet<String>, EngineError> {
    let found = stale_in(repo, snapshot, Date::today())?;
    Ok(found.into_iter().map(|(id, _)| id).collect())
}

/// The overview and the features, in the catalog's order.
pub fn product(repo: &Repo) -> Result<Product, EngineError> {
    let snapshot = repo.snapshot()?;
    let updated = updated_days(repo)?;
    let catalog = Catalog::read(repo, &snapshot);
    let members = catalog.members();
    let stale = stale_ids(repo, &snapshot)?;
    let features = catalog
        .features
        .iter()
        .map(|doc| {
            let of = members.get(doc.0.id.as_str()).map_or(&[][..], Vec::as_slice);
            catalog.row(&repo.store.config, *doc, &updated, of, &stale)
        })
        .collect();
    let overview = catalog.overview.map(|doc| ProductOverview {
        id: snapshot.label(doc.0),
        title: title_of(&snapshot, doc),
        summary: summary_of(doc.1),
    });
    Ok(Product { overview, features })
}

/// The features of one task, in the catalog's order; the answer of `GET /api/tasks/{id}/features`.
pub fn task_features(repo: &Repo, input: &str) -> Result<Vec<TaskFeature>, EngineError> {
    let snapshot = repo.snapshot()?;
    let (entry, _) = snapshot.resolve(input)?;
    if entry.item.as_ref().and_then(Item::item_type) != Some(ItemType::Task) {
        return Err(EngineError::NotATask {
            id: snapshot.label(entry),
        });
    }
    let catalog = Catalog::read(repo, &snapshot);
    let members = catalog.members();
    let found = catalog.features.iter().filter_map(|doc| {
        let of = members.get(doc.0.id.as_str())?;
        let member = of.iter().find(|member| member.task.0.id == entry.id)?;
        Some(TaskFeature {
            id: snapshot.label(doc.0),
            title: title_of(&snapshot, *doc),
            via: member.via.clone(),
        })
    });
    Ok(found.collect())
}

/// One feature and what it connects to. `input` is an item id or a doc's name.
pub fn feature(repo: &Repo, input: &str) -> Result<FeatureDetail, EngineError> {
    let snapshot = repo.snapshot()?;
    let entry = match snapshot.resolve(input) {
        Ok((entry, _)) => entry,
        Err(err) => match snapshot.resolve(&format!("docs/{input}")) {
            Ok((entry, _)) => entry,
            Err(_) => return Err(err),
        },
    };
    let updated = updated_days(repo)?;
    let catalog = Catalog::read(repo, &snapshot);
    let Some(doc) = catalog.features.iter().find(|(e, _)| e.id == entry.id).copied() else {
        return Err(EngineError::NotFound {
            id: input.to_string(),
            suggestion: None,
        });
    };
    let members = catalog.members();
    let of = members.get(entry.id.as_str()).map_or(&[][..], Vec::as_slice);
    let work = WorkLinks::load(repo, &snapshot)?;

    let mut task_rows: Vec<FeatureTask> = of
        .iter()
        .map(|member| {
            let (task, item) = member.task;
            let closed = work.closed(task, item);
            FeatureTask {
                id: snapshot.label(task),
                title: title_of(&snapshot, member.task),
                status: item.fields.status.clone(),
                inferred: member.via.is_some(),
                via: member.via.clone(),
                closed_estimated: closed.as_ref().is_some_and(|c| c.estimated),
                closed: closed.map(|c| c.day),
            }
        })
        .collect();
    task_rows.sort_by(|a, b| a.inferred.cmp(&b.inferred).then_with(|| a.id.cmp(&b.id)));

    // Linked tasks first: a commit two tasks share counts for the one that says the most.
    let mut seen = HashSet::new();
    let by_linked = of.iter().filter(|m| m.via.is_none());
    let mut commits: Vec<FeatureCommit> = by_linked
        .chain(of.iter().filter(|m| m.via.is_some()))
        .flat_map(|member| {
            let task = snapshot.label(member.task.0);
            let inferred = member.via.is_some();
            work.commits_of(member.task.0.id.as_str())
                .into_iter()
                .map(move |commit| FeatureCommit {
                    commit,
                    task: task.clone(),
                    inferred,
                })
        })
        .filter(|row| seen.insert((row.commit.repo.clone(), row.commit.sha.clone())))
        .collect();
    commits.sort_by(|a, b| {
        b.commit
            .date
            .cmp(&a.commit.date)
            .then_with(|| a.commit.sha.cmp(&b.commit.sha))
    });
    commits.truncate(MAX_COMMITS);

    Ok(FeatureDetail {
        feature: catalog.row(&repo.store.config, doc, &updated, of, &stale_ids(repo, &snapshot)?),
        links: FeatureLinks {
            docs: catalog.docs_of(entry),
            tasks: task_rows,
            parts: catalog.parts_of(doc, of),
            commits,
        },
    })
}
