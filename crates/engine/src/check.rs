//! `stone check`: one pass over the whole store that reports every problem.

use std::collections::BTreeMap;
use std::fmt;

use carvestone_core::{ItemType, NumberedKind};
use carvestone_store::{Entry, FrontmatterError};
use serde::Serialize;

use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::timeline::Date;

const TYPES: &str = "doc, plan, task, milestone, decision, reference, system, container, component";

/// How much a finding matters: an error fails `stone check`, a warning only with `--strict`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

impl Severity {
    fn is_error(&self) -> bool {
        *self == Severity::Error
    }
}

/// One finding, printed as `<path>:<line>: <rule>: <message>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Problem {
    pub path: String,
    pub line: usize,
    pub rule: &'static str,
    pub message: String,
    /// Left out of the JSON for errors.
    #[serde(skip_serializing_if = "Severity::is_error")]
    #[cfg_attr(feature = "ts", ts(as = "Option<_>", optional))]
    pub severity: Severity,
}

impl Problem {
    pub fn new(path: &str, line: usize, rule: &'static str, message: impl Into<String>) -> Problem {
        Problem {
            path: path.to_string(),
            line,
            rule,
            message: message.into(),
            severity: Severity::Error,
        }
    }

    pub fn warning(path: &str, line: usize, rule: &'static str, message: impl Into<String>) -> Problem {
        Problem {
            severity: Severity::Warning,
            ..Problem::new(path, line, rule, message)
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}: {}", self.path, self.line, self.rule, self.message)
    }
}

impl Repo {
    /// Every problem in the store, sorted by path, line and rule.
    pub fn check(&self) -> Result<Vec<Problem>, EngineError> {
        self.check_on(Date::today())
    }

    /// `check` with the day staleness is judged on, so tests do not depend on the clock.
    pub fn check_on(&self, today: Date) -> Result<Vec<Problem>, EngineError> {
        let snapshot = self.snapshot()?;
        let mut out = Vec::new();
        for entry in &snapshot.entries {
            entry_problems(&snapshot, entry, &mut out);
        }
        duplicate_ids(&snapshot, &mut out);
        crate::check_fields::field_problems(&self.store.config, &snapshot, &mut out);
        crate::check_fields::uses_problems(&snapshot, &mut out);
        let reserved: Vec<String> = self.store.scan_reserved()?.into_iter().map(|(path, _)| path).collect();
        let docs = self.store.scan_docs()?;
        crate::check_links::link_problems(&snapshot, &docs, &self.store.config, &reserved, &mut out);
        crate::check_files::file_problems(&self.store, &snapshot, &mut out)?;
        out.extend(self.ownership(&snapshot)?);
        self.code_problems(&snapshot, &docs, &mut out)?;
        // A repo git cannot read is skipped by every git answer, so check is where it gets said.
        if let Ok(report) = crate::refresh::refresh(self) {
            for message in report.git_broken {
                out.push(Problem::warning("stone/config.yml", 1, "git", message));
            }
        }
        crate::agent_block::block_problems(&self.store.layout.root, &mut out);
        crate::guide::guide_problems(&snapshot, &mut out);
        for (path, stale) in crate::stale::stale_paths(self, &snapshot, &docs, today)? {
            out.push(Problem::warning(&path, 1, "stale", stale.reason));
        }
        out.sort_by(|a, b| (&a.path, a.line, a.rule).cmp(&(&b.path, b.line, b.rule)));
        Ok(out)
    }
}

impl Repo {
    /// Every parse error in the code index, one finding per site. `check` gives only the count and the first few.
    pub fn syntax_sites(&self) -> Result<Vec<Problem>, EngineError> {
        crate::code_index::syntax_sites(crate::code::open(self)?.conn())
    }

    /// Code links and component drift, both read from the code index. A checkout that cannot hold
    /// `.stone/` skips them, like `get` skips backlinks.
    fn code_problems(
        &self,
        snapshot: &Snapshot,
        docs: &[carvestone_store::Entry],
        out: &mut Vec<Problem>,
    ) -> Result<(), EngineError> {
        let index = match crate::code::open(self) {
            Ok(index) => index,
            Err(EngineError::Invalid(msg)) if msg.starts_with("cannot write .stone/") => return Ok(()),
            Err(err) => return Err(err),
        };
        let conn = index.conn();
        crate::check_links::code_link_problems(&self.store.layout.root, snapshot, docs, conn, out)?;
        crate::arch_graph::drift_problems(&self.store.layout.root, snapshot, conn, out)?;
        crate::code_entries::entry_problems(snapshot, conn, out)?;
        crate::design::flow_problems(snapshot, docs, conn, &self.store.layout.root, out)?;
        crate::code_index::syntax_problems(conn, out)
    }
}

fn entry_problems(snapshot: &Snapshot, entry: &Entry, out: &mut Vec<Problem>) {
    let path = entry.path.as_str();
    let doc = match &entry.doc {
        Err(e) => {
            let line = match e {
                FrontmatterError::Yaml(y) => y.location().map_or(1, |l| l.line() + 1),
                _ => 1,
            };
            out.push(Problem::new(path, line, "frontmatter", e.to_string()));
            return;
        }
        Ok(doc) => doc,
    };
    for (line, reason) in doc.subset_problems() {
        out.push(Problem::new(path, line, "subset", reason));
    }
    let Some(item) = &entry.item else { return };

    if item.fields.title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        out.push(Problem::new(
            path,
            doc.key_line("title").unwrap_or(1),
            "title",
            "add `title:` with a short name",
        ));
    }

    let type_line = doc.key_line("type").unwrap_or(1);
    let Some(name) = &item.type_name else {
        out.push(Problem::new(path, 1, "type", format!("add `type:`; one of {TYPES}")));
        return;
    };
    let Some(ty) = ItemType::from_name(name) else {
        out.push(Problem::new(
            path,
            type_line,
            "type",
            format!("unknown type `{name}`; one of {TYPES}"),
        ));
        return;
    };

    let folder = entry.id.as_str().split_once('/').map(|(f, _)| f);
    if folder != Some(ty.folder()) {
        out.push(Problem::new(
            path,
            type_line,
            "folder",
            format!("a {name} belongs in stone/{}/", ty.folder()),
        ));
        return;
    }

    let kind = match ty {
        ItemType::Task => NumberedKind::Task,
        ItemType::Milestone => NumberedKind::Milestone,
        ItemType::Decision => NumberedKind::Decision,
        _ => return,
    };
    let scheme = &snapshot.scheme;
    let canonical = scheme
        .number(&entry.id)
        .filter(|(k, _)| *k == kind)
        .map(|(_, n)| scheme.numbered_id(kind, n));
    match canonical {
        Some(id) if id == entry.id => {}
        Some(id) => {
            let file = id.as_str().rsplit('/').next().unwrap_or_default();
            out.push(Problem::new(path, 1, "file-name", format!("name the file {file}.md")));
        }
        None => {
            let pattern = match kind {
                NumberedKind::Task => format!("{}-<number>.md", scheme.task_prefix),
                NumberedKind::Milestone => "M-<number>.md".to_string(),
                NumberedKind::Decision => "ADR-<number>.md".to_string(),
            };
            out.push(Problem::new(path, 1, "file-name", format!("name the file {pattern}")));
        }
    }
}

fn duplicate_ids(snapshot: &Snapshot, out: &mut Vec<Problem>) {
    let mut groups: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for entry in &snapshot.entries {
        let key = snapshot
            .scheme
            .short(&entry.id)
            .unwrap_or_else(|| entry.id.as_str().to_string())
            .to_lowercase();
        groups.entry(key).or_default().push(entry);
    }
    for members in groups.values().filter(|m| m.len() > 1) {
        let paths: Vec<&str> = members.iter().map(|e| e.path.as_str()).collect();
        let message = format!(
            "same id as {}; give one a new number (`stone renumber <path>`) or remove one",
            paths.join(", ")
        );
        for entry in members {
            out.push(Problem::new(&entry.path, 1, "duplicate-id", message.clone()));
        }
    }
}
