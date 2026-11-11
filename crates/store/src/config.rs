//! The repo's `stone/config.yml`.

use std::path::Path;

use carvestone_core::{IdScheme, ItemType};
use serde_norway::{Mapping, Value};

use crate::store::StoreError;

/// Status names that say work is waiting, wherever a project's workflow puts them.
const WAITING: [&str; 10] = [
    "draft", "backlog", "todo", "to-do", "planned", "new", "open", "idea", "proposed", "ready",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub project: String,
    pub task_prefix: String,
    pub statuses: Vec<String>,
    pub milestone_statuses: Vec<String>,
    pub plan_statuses: Vec<String>,
    pub decision_statuses: Vec<String>,
    pub task_kinds: Vec<String>,
    pub priorities: Vec<String>,
    pub definition_of_done: Vec<String>,
    pub code_roots: Vec<String>,
    /// Plain markdown folders, indexed read-only for search and links.
    pub doc_roots: Vec<String>,
    /// Commits touching an item's code after its last check that make the item stale.
    pub stale_commits: u32,
    /// When true, a task moves to done only with a `## Plan` and an `## Outcome` section. Off by default.
    pub done_needs_plan: bool,
    /// SCIP index files the user or CI builds. Data only: config never names a command to run.
    pub scip_indexes: Vec<ScipIndexEntry>,
}

/// One `scip_indexes` entry: an index file, root-relative, and the files it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipIndexEntry {
    pub file: String,
    pub language: String,
    /// File extensions of the language, without a dot.
    pub extensions: Vec<String>,
}

/// `stale_commits` when `config.yml` does not set it.
pub const DEFAULT_STALE_COMMITS: u32 = 20;

/// The files a project keeps its own rules and overview in. Each one directly in the root or in a
/// code root is read as a plain doc without a `doc_roots` entry; the name matches in any case.
pub const PROJECT_FILES: [&str; 3] = ["README.md", "CLAUDE.md", "AGENTS.md"];

impl Config {
    pub fn load(path: &Path) -> Result<Config, StoreError> {
        let text = std::fs::read_to_string(path).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Config::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Config, StoreError> {
        let map = match serde_norway::from_str::<Value>(text).map_err(|e| StoreError::Config(e.to_string()))? {
            Value::Mapping(m) => m,
            Value::Null => Mapping::new(),
            _ => {
                return Err(StoreError::Config("stone/config.yml must be a mapping of keys".into()));
            }
        };
        let project = match map.get("project") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let task_prefix = match map.get("task_prefix") {
            Some(Value::String(s)) => s.clone(),
            _ => "TASK".to_string(),
        };
        check_prefix(&task_prefix)?;
        let doc_roots = list(&map, "doc_roots", &[])?;
        check_doc_roots(&doc_roots)?;
        Ok(Config {
            project,
            task_prefix,
            statuses: list(
                &map,
                "statuses",
                &["draft", "todo", "in-progress", "review", "done", "archived"],
            )?,
            milestone_statuses: list(&map, "milestone_statuses", &["planned", "active", "done"])?,
            plan_statuses: list(&map, "plan_statuses", &["draft", "active", "done"])?,
            decision_statuses: list(&map, "decision_statuses", &["proposed", "accepted", "superseded"])?,
            task_kinds: list(&map, "task_kinds", &["feature", "chore", "spike", "bug"])?,
            priorities: list(&map, "priorities", &["high", "medium", "low"])?,
            definition_of_done: list(&map, "definition_of_done", &[])?,
            code_roots: list(&map, "code_roots", &[])?,
            doc_roots,
            stale_commits: count(&map, "stale_commits", DEFAULT_STALE_COMMITS)?,
            done_needs_plan: flag(&map, "done_needs_plan")?,
            scip_indexes: scip_indexes(&map)?,
        })
    }

    /// The doc roots as root-relative folders: `/` separators, no `./`, no trailing slash.
    pub fn doc_folders(&self) -> Vec<String> {
        self.doc_roots.iter().map(|r| folder_of(r)).collect()
    }

    /// The doc folder that holds `root_rel`, a root-relative file or folder path; for a project
    /// file outside every doc folder, the project folder it sits in (`""` for the root).
    pub fn doc_root_of(&self, root_rel: &str) -> Option<String> {
        let path = folder_of(root_rel);
        self.doc_folders()
            .into_iter()
            .find(|folder| path == *folder || path.starts_with(&format!("{folder}/")))
            .or_else(|| self.project_folder_of(&path))
    }

    /// The folders whose project files are read: the root (`""`) and every code root, without
    /// repeats and never `stone/`.
    pub fn project_folders(&self) -> Vec<String> {
        let mut out = vec![String::new()];
        for root in &self.code_roots {
            let folder = folder_of(root);
            let folder = if folder == "." { String::new() } else { folder };
            if folder != "stone" && !folder.starts_with("stone/") && !out.contains(&folder) {
                out.push(folder);
            }
        }
        out
    }

    /// The project folder of `root_rel` when it names one of `PROJECT_FILES` directly in it.
    fn project_folder_of(&self, root_rel: &str) -> Option<String> {
        let (folder, name) = root_rel.rsplit_once('/').unwrap_or(("", root_rel));
        let named = PROJECT_FILES.iter().any(|file| file.eq_ignore_ascii_case(name));
        let folders = self.project_folders();
        (named && folders.iter().any(|f| f == folder)).then(|| folder.to_string())
    }

    /// True when `folder` is a configured doc folder, not a project folder.
    pub fn is_doc_folder(&self, folder: &str) -> bool {
        self.doc_folders().iter().any(|f| f == folder)
    }

    pub fn scheme(&self) -> IdScheme {
        IdScheme {
            task_prefix: self.task_prefix.clone(),
        }
    }

    /// The task statuses of work nobody has started. `statuses` lists the workflow in order, so this
    /// is everything up to the last status with a waiting name, or the first status when none has.
    pub fn waiting_statuses(&self) -> &[String] {
        let last = self
            .statuses
            .iter()
            .rposition(|status| WAITING.contains(&status.as_str()));
        let end = last.unwrap_or(0).saturating_add(1).min(self.statuses.len());
        &self.statuses[..end]
    }

    /// The task statuses of work that is over: `done` and what follows it, or the last status of a
    /// project with no `done`.
    pub fn finished_statuses(&self) -> &[String] {
        let done = self.statuses.iter().position(|status| status == "done");
        &self.statuses[done.unwrap_or_else(|| self.statuses.len().saturating_sub(1))..]
    }

    /// Whether a task with this status has been started, finished ones included.
    pub fn task_started(&self, status: &str) -> bool {
        self.statuses.iter().any(|known| known == status) && !self.waiting_statuses().iter().any(|s| s == status)
    }

    pub fn task_finished(&self, status: &str) -> bool {
        self.finished_statuses().iter().any(|known| known == status)
    }

    pub fn statuses_for(&self, t: ItemType) -> Vec<String> {
        match t {
            ItemType::Task => self.statuses.clone(),
            ItemType::Milestone => self.milestone_statuses.clone(),
            ItemType::Plan => self.plan_statuses.clone(),
            ItemType::Decision => self.decision_statuses.clone(),
            ItemType::Doc | ItemType::Reference | ItemType::System | ItemType::Container | ItemType::Component => {
                ["draft", "stable", "deprecated"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            }
        }
    }
}

/// `path` with `/` separators, without a leading `./` or `/` and without a trailing slash.
fn folder_of(path: &str) -> String {
    let flat = path.trim().replace('\\', "/");
    let mut rest = flat.as_str();
    while let Some(tail) = rest.strip_prefix("./").or_else(|| rest.strip_prefix('/')) {
        rest = tail;
    }
    rest.trim_end_matches('/').to_string()
}

/// A doc root is a folder below the root that is not `stone/` itself: stone owns that folder
/// and never reads it as a plain doc folder.
fn check_doc_roots(roots: &[String]) -> Result<(), StoreError> {
    for root in roots {
        let folder = folder_of(root);
        let bad = |why: &str| StoreError::Config(format!("invalid `doc_roots` entry {root:?}: {why}"));
        if folder.is_empty() || folder == "." {
            return Err(bad("name a folder below the root, such as `docs/`"));
        }
        if folder.split('/').any(|seg| seg == ".." || seg == ".") || folder.contains(':') {
            return Err(bad("write a path inside the root, such as `docs/`"));
        }
        if folder == "stone" || folder.starts_with("stone/") {
            return Err(bad("stone/ holds items, not plain docs; list a folder outside it"));
        }
    }
    Ok(())
}

/// Milestone and decision short ids use `M` and `ADR`, so a task prefix may not.
fn check_prefix(prefix: &str) -> Result<(), StoreError> {
    let mut chars = prefix.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric());
    if !valid {
        return Err(StoreError::Config(format!(
            "invalid `task_prefix` {prefix:?}: use ASCII letters and digits, starting with a letter"
        )));
    }
    if prefix.eq_ignore_ascii_case("M") || prefix.eq_ignore_ascii_case("ADR") {
        return Err(StoreError::Config(format!(
            "`task_prefix` {prefix:?} is reserved for milestone and decision ids"
        )));
    }
    Ok(())
}

/// The `scip_indexes` list. Every entry needs `file`, `language` and a non-empty `extensions`;
/// any other key is refused, so a `command` key can never be read and never runs.
fn scip_indexes(map: &Mapping) -> Result<Vec<ScipIndexEntry>, StoreError> {
    let items = match map.get("scip_indexes") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Sequence(items)) => items,
        Some(_) => {
            return Err(StoreError::Config(
                "`scip_indexes` must be a list of entries with `file`, `language` and `extensions`".into(),
            ));
        }
    };
    let mut out = Vec::new();
    for (n, item) in items.iter().enumerate() {
        let bad = |why: &str| StoreError::Config(format!("invalid `scip_indexes` entry {}: {why}", n + 1));
        let Value::Mapping(entry) = item else {
            return Err(bad("write `file`, `language` and `extensions`"));
        };
        for key in entry.keys() {
            if !matches!(key.as_str(), Some("file" | "language" | "extensions")) {
                let name = key
                    .as_str()
                    .map_or("(not text)".to_string(), |name| format!("`{name}`"));
                return Err(bad(&format!(
                    "unknown key {name}; only `file`, `language` and `extensions` are allowed, and config never holds a command"
                )));
            }
        }
        let text = |key: &str| match entry.get(key) {
            Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            Some(_) => Err(bad(&format!("`{key}` must be a non-empty string"))),
            None => Err(bad(&format!("add the `{key}` key"))),
        };
        let file = text("file")?;
        // A repo's config is not trusted, so it may not point a read outside the repo.
        let flat = file.replace('\\', "/");
        let drive = flat.as_bytes().get(1) == Some(&b':');
        if flat.starts_with('/') || drive || flat.split('/').any(|seg| seg == "..") {
            return Err(bad("`file` must be a path inside the repo, such as `.scip/java.scip`"));
        }
        let language = text("language")?;
        // Stone stores project files under this name, so a source language may not take it.
        if language.eq_ignore_ascii_case("project") {
            return Err(bad("`language` cannot be `project`; name the language, such as `java`"));
        }
        let extensions = match entry.get("extensions") {
            None => return Err(bad("add the `extensions` key")),
            Some(Value::Sequence(items)) if !items.is_empty() => items
                .iter()
                .map(|v| match v {
                    Value::String(s) if !s.is_empty() && !s.contains('.') => Ok(s.clone()),
                    _ => Err(bad(
                        "each extension is a non-empty string without a dot, such as `java`",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(bad("`extensions` must be a non-empty list of strings")),
        };
        out.push(ScipIndexEntry {
            file,
            language,
            extensions,
        });
    }
    Ok(out)
}

/// `true` or `false`; off when the key is absent.
fn flag(map: &Mapping, key: &str) -> Result<bool, StoreError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(StoreError::Config(format!("`{key}` must be true or false"))),
    }
}

/// A whole number of at least 1.
fn count(map: &Mapping, key: &str, default: u32) -> Result<u32, StoreError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => n
            .as_u64()
            .filter(|n| *n >= 1)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| StoreError::Config(format!("`{key}` must be a whole number of at least 1"))),
        Some(_) => Err(StoreError::Config(format!(
            "`{key}` must be a whole number of at least 1"
        ))),
    }
}

fn list(map: &Mapping, key: &str, default: &[&str]) -> Result<Vec<String>, StoreError> {
    let bad = || StoreError::Config(format!("`{key}` must be a list of strings"));
    match map.get(key) {
        None | Some(Value::Null) => Ok(default.iter().map(|s| s.to_string()).collect()),
        Some(Value::Sequence(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(s.clone()),
                _ => Err(bad()),
            })
            .collect(),
        Some(_) => Err(bad()),
    }
}
