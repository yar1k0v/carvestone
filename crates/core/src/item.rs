//! Item envelope: the typed frontmatter keys, provenance and trust.
//!
//! One flat `Fields` struct holds every key of every type; per-type rules live
//! in `stone check`. Keys Carvestone does not know stay in the file text and
//! never enter this model. Trust is derived from `verified`, never stored.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::id::{ItemId, ItemType};

/// Who did something and when (OKF `generated` / `verified` shape). Both are
/// kept as written; `trust` and `stone check` judge them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub by: String,
    /// ISO-8601 date or datetime as text. The YAML parser has no timestamp type,
    /// so an unquoted `2026-10-16` reads as this string.
    pub at: String,
}

/// OKF v0.2 actor: `human:<id>`, `process:<id>` or `<producer>[/<version>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Human(String),
    Process(String),
    Producer { name: String, version: Option<String> },
}

fn is_part(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl FromStr for Actor {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const HINT: &str =
            "expected `human:<id>`, `process:<id>` or `<producer>[/<version>]` made of letters, digits, `.`, `_`, `-`";
        if let Some(id) = s.strip_prefix("human:") {
            return if is_part(id) {
                Ok(Actor::Human(id.to_string()))
            } else {
                Err(Error::InvalidActor(s.into(), HINT))
            };
        }
        if let Some(id) = s.strip_prefix("process:") {
            return if is_part(id) {
                Ok(Actor::Process(id.to_string()))
            } else {
                Err(Error::InvalidActor(s.into(), HINT))
            };
        }
        let (name, version) = match s.split_once('/') {
            Some((n, v)) => (n, Some(v)),
            None => (s, None),
        };
        if !is_part(name) || version.is_some_and(|v| !is_part(v)) {
            return Err(Error::InvalidActor(s.into(), HINT));
        }
        Ok(Actor::Producer {
            name: name.to_string(),
            version: version.map(str::to_string),
        })
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::Human(id) => write!(f, "human:{id}"),
            Actor::Process(id) => write!(f, "process:{id}"),
            Actor::Producer { name, version: None } => write!(f, "{name}"),
            Actor::Producer { name, version: Some(v) } => write!(f, "{name}/{v}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum Trust {
    Unverified,
    MachineConfirmed,
    HumanReviewed,
}

/// No verifier: unverified. Any valid `human:` verifier: human-reviewed.
/// Otherwise machine-confirmed. An unparsable `by` is never a human.
pub fn trust(verified: &[Stamp]) -> Trust {
    if verified.is_empty() {
        return Trust::Unverified;
    }
    if verified
        .iter()
        .any(|s| matches!(s.by.parse::<Actor>(), Ok(Actor::Human(_))))
    {
        Trust::HumanReviewed
    } else {
        Trust::MachineConfirmed
    }
}

pub const LIST_KEYS: [&str; 9] = [
    "tags",
    "links",
    "depends_on",
    "components",
    "paths",
    "code",
    "entry",
    "uses",
    "libraries",
];

/// The design levels a doc may name in `level`.
pub const LEVELS: [&str; 2] = ["hld", "lld"];

/// The `kind` values a system may carry: a person, or a system outside the product.
pub const SYSTEM_KINDS: [&str; 2] = ["person", "external"];

/// The `kind` values a container may carry: a data store or a message bus.
pub const CONTAINER_KINDS: [&str; 1] = ["store"];

pub const SCALAR_KEYS: [&str; 20] = [
    "title",
    "description",
    "status",
    "created",
    "closed",
    "stale_after",
    "priority",
    "kind",
    "milestone",
    "plan",
    "assignee",
    "target",
    "supersedes",
    "resource",
    "library",
    "version",
    "order",
    "technology",
    "parent",
    "level",
];

/// Every typed frontmatter key of every item type. `type` lives in
/// `Item.type_name`; unknown keys are never stored here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Fields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The day a task was done; `stone task move` writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// A string so `1.40` never becomes `1.4`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technology: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The design level of a doc: `hld` or `lld`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub code: Vec<String>,
    /// Where execution of an arch element starts: `path#Symbol` or `path`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entry: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated: Option<Stamp>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub verified: Vec<Stamp>,
}

impl Fields {
    pub fn list_mut(&mut self, key: &str) -> Option<&mut Vec<String>> {
        match key {
            "tags" => Some(&mut self.tags),
            "links" => Some(&mut self.links),
            "depends_on" => Some(&mut self.depends_on),
            "components" => Some(&mut self.components),
            "paths" => Some(&mut self.paths),
            "code" => Some(&mut self.code),
            "entry" => Some(&mut self.entry),
            "uses" => Some(&mut self.uses),
            "libraries" => Some(&mut self.libraries),
            _ => None,
        }
    }

    pub fn scalar_mut(&mut self, key: &str) -> Option<&mut Option<String>> {
        match key {
            "title" => Some(&mut self.title),
            "description" => Some(&mut self.description),
            "status" => Some(&mut self.status),
            "created" => Some(&mut self.created),
            "closed" => Some(&mut self.closed),
            "stale_after" => Some(&mut self.stale_after),
            "priority" => Some(&mut self.priority),
            "kind" => Some(&mut self.kind),
            "milestone" => Some(&mut self.milestone),
            "plan" => Some(&mut self.plan),
            "assignee" => Some(&mut self.assignee),
            "target" => Some(&mut self.target),
            "supersedes" => Some(&mut self.supersedes),
            "resource" => Some(&mut self.resource),
            "library" => Some(&mut self.library),
            "version" => Some(&mut self.version),
            "order" => Some(&mut self.order),
            "technology" => Some(&mut self.technology),
            "parent" => Some(&mut self.parent),
            "level" => Some(&mut self.level),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: ItemId,
    pub type_name: Option<String>,
    pub fields: Fields,
    pub body: String,
    /// The file line (1-based) of the body's first line.
    pub body_line: usize,
}

impl Item {
    pub fn item_type(&self) -> Option<ItemType> {
        self.type_name.as_deref().and_then(ItemType::from_name)
    }
}
