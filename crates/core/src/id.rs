//! Item ids and the short forms people type.
//!
//! An id is the item's path under `stone/` without `.md`, `/` separators:
//! `tasks/CS-7`, `docs/prd`, `arch/carvestone/core`. Numbered items also answer
//! to a short id (`CS-7`, `M-3`, `ADR-7`), matched case-insensitively with zero
//! padding ignored and always printed in canonical form.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The `type` a document declares; decides which folder it lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Doc,
    Plan,
    Task,
    Milestone,
    Decision,
    Reference,
    System,
    Container,
    Component,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Doc => "doc",
            ItemType::Plan => "plan",
            ItemType::Task => "task",
            ItemType::Milestone => "milestone",
            ItemType::Decision => "decision",
            ItemType::Reference => "reference",
            ItemType::System => "system",
            ItemType::Container => "container",
            ItemType::Component => "component",
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            ItemType::Doc => "docs",
            ItemType::Plan => "plans",
            ItemType::Task => "tasks",
            ItemType::Milestone => "milestones",
            ItemType::Decision => "decisions",
            ItemType::Reference => "refs",
            ItemType::System | ItemType::Container | ItemType::Component => "arch",
        }
    }

    /// Inverse of `as_str`; exact, lowercase only.
    pub fn from_name(name: &str) -> Option<ItemType> {
        match name {
            "doc" => Some(ItemType::Doc),
            "plan" => Some(ItemType::Plan),
            "task" => Some(ItemType::Task),
            "milestone" => Some(ItemType::Milestone),
            "decision" => Some(ItemType::Decision),
            "reference" => Some(ItemType::Reference),
            "system" => Some(ItemType::System),
            "container" => Some(ItemType::Container),
            "component" => Some(ItemType::Component),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NumberedKind {
    Task,
    Milestone,
    Decision,
}

impl NumberedKind {
    fn folder(self) -> &'static str {
        match self {
            NumberedKind::Task => "tasks",
            NumberedKind::Milestone => "milestones",
            NumberedKind::Decision => "decisions",
        }
    }
}

/// Path of an item under `stone/` without `.md`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    /// No validation: callers that need it go through `IdScheme::parse`.
    pub fn from_path(path: &str) -> ItemId {
        let path = path.replace('\\', "/");
        ItemId(path.strip_suffix(".md").unwrap_or(&path).to_string())
    }

    /// The root-relative path of a read-only doc file, `.md` kept: store ids never carry one, so
    /// the two kinds cannot collide.
    pub fn from_file(path: &str) -> ItemId {
        ItemId(path.replace('\\', "/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An id plus an optional section anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub id: ItemId,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdScheme {
    pub task_prefix: String,
}

const FOLDERS: [&str; 7] = ["docs", "plans", "tasks", "milestones", "decisions", "refs", "arch"];

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_segment(s: &str) -> bool {
    !matches!(s, "" | "." | "..")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl IdScheme {
    pub fn parse(&self, input: &str) -> Result<Ref, Error> {
        let (body, anchor) = match input.split_once('#') {
            Some((_, "")) => return Err(Error::InvalidId(input.into(), "empty anchor")),
            Some((b, a)) => (b, Some(a.to_string())),
            None => (input, None),
        };
        let body = body.strip_prefix('/').unwrap_or(body);
        let body = body.strip_suffix(".md").unwrap_or(body);
        let unknown = || Error::InvalidId(input.into(), "unknown id form");

        let id = if let Some((folder, rest)) = body.split_once('/') {
            if !FOLDERS.contains(&folder) {
                return Err(unknown());
            }
            let numbered = [NumberedKind::Task, NumberedKind::Milestone, NumberedKind::Decision]
                .into_iter()
                .find(|k| k.folder() == folder);
            match numbered {
                Some(want) => {
                    if rest.contains('/') {
                        return Err(unknown());
                    }
                    let (kind, n) = self.parse_short(rest, input)?;
                    if kind != want {
                        return Err(unknown());
                    }
                    self.numbered_id(kind, n)
                }
                None => {
                    if !rest.split('/').all(is_segment) {
                        return Err(Error::InvalidId(input.into(), "invalid path segment"));
                    }
                    ItemId(body.to_string())
                }
            }
        } else {
            let (kind, n) = self.parse_short(body, input)?;
            self.numbered_id(kind, n)
        };
        Ok(Ref { id, anchor })
    }

    fn parse_short(&self, s: &str, input: &str) -> Result<(NumberedKind, u32), Error> {
        let unknown = || Error::InvalidId(input.into(), "unknown id form");
        let (head, num) = match s.rsplit_once('-') {
            Some((h, n)) if !h.is_empty() => (h, n),
            _ => return Err(unknown()),
        };
        if !is_digits(num) {
            return Err(Error::InvalidId(input.into(), "expected a number after '-'"));
        }
        let n = num
            .parse::<u32>()
            .map_err(|_| Error::InvalidId(input.into(), "number too large"))?;
        let kind = if head.eq_ignore_ascii_case("m") {
            NumberedKind::Milestone
        } else if head.eq_ignore_ascii_case("adr") {
            NumberedKind::Decision
        } else if head.eq_ignore_ascii_case(&self.task_prefix) {
            NumberedKind::Task
        } else {
            return Err(unknown());
        };
        Ok((kind, n))
    }

    pub fn numbered_id(&self, kind: NumberedKind, n: u32) -> ItemId {
        match kind {
            NumberedKind::Task => ItemId(format!("tasks/{}-{n}", self.task_prefix)),
            NumberedKind::Milestone => ItemId(format!("milestones/M-{n}")),
            NumberedKind::Decision => ItemId(format!("decisions/ADR-{n}")),
        }
    }

    /// The kind and number of a numbered item's path; zero padding is accepted
    /// so `stone check` can flag a non-canonical file name.
    pub fn number(&self, id: &ItemId) -> Option<(NumberedKind, u32)> {
        let (folder, rest) = id.as_str().split_once('/')?;
        if rest.contains('/') {
            return None;
        }
        let (head, num) = rest.rsplit_once('-')?;
        let (kind, want) = match folder {
            "tasks" => (NumberedKind::Task, self.task_prefix.as_str()),
            "milestones" => (NumberedKind::Milestone, "M"),
            "decisions" => (NumberedKind::Decision, "ADR"),
            _ => return None,
        };
        if !head.eq_ignore_ascii_case(want) || !is_digits(num) {
            return None;
        }
        Some((kind, num.parse().ok()?))
    }

    pub fn short(&self, id: &ItemId) -> Option<String> {
        let (kind, n) = self.number(id)?;
        Some(match kind {
            NumberedKind::Task => format!("{}-{n}", self.task_prefix),
            NumberedKind::Milestone => format!("M-{n}"),
            NumberedKind::Decision => format!("ADR-{n}"),
        })
    }

    pub fn link_text(&self, r: &Ref) -> String {
        let base = self.short(&r.id).unwrap_or_else(|| r.id.as_str().to_string());
        match &r.anchor {
            Some(a) => format!("{base}#{a}"),
            None => base,
        }
    }
}
