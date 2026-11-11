//! What a form needs to edit an item: the statuses and priorities this repo allows and the
//! fields each item type carries. One serde struct, shared by HTTP and anything else.

use std::collections::BTreeMap;

use carvestone_core::ItemType;
use serde::Serialize;

use crate::repo::Repo;

/// One editable frontmatter field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FieldSpec {
    pub key: String,
    /// `text`, `date`, `number`, `url`, `enum`, `ref` (one item id) or `list` (comma-separated items or words).
    pub kind: String,
    /// The allowed values of an `enum`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub values: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApiConfig {
    pub task_prefix: String,
    /// Allowed `status` values per item type name.
    pub statuses: BTreeMap<String, Vec<String>>,
    pub priorities: Vec<String>,
    pub task_kinds: Vec<String>,
    /// Editable fields per item type name, in the order a form shows them.
    pub fields: BTreeMap<String, Vec<FieldSpec>>,
}

const TYPES: [ItemType; 9] = [
    ItemType::Doc,
    ItemType::Plan,
    ItemType::Task,
    ItemType::Milestone,
    ItemType::Decision,
    ItemType::Reference,
    ItemType::System,
    ItemType::Container,
    ItemType::Component,
];

fn spec(key: &str, kind: &str) -> FieldSpec {
    FieldSpec {
        key: key.to_string(),
        kind: kind.to_string(),
        values: None,
    }
}

fn choice(key: &str, values: Vec<String>) -> FieldSpec {
    FieldSpec {
        values: Some(values),
        ..spec(key, "enum")
    }
}

/// The fields of one type, ending with the ones every type shares.
fn fields_of(repo: &Repo, ty: ItemType) -> Vec<FieldSpec> {
    let config = &repo.store.config;
    let mut out = vec![
        spec("title", "text"),
        spec("description", "text"),
        choice("status", config.statuses_for(ty)),
    ];
    match ty {
        ItemType::Task => out.extend([
            choice("priority", config.priorities.clone()),
            choice("kind", config.task_kinds.clone()),
            spec("milestone", "ref"),
            spec("plan", "ref"),
            spec("assignee", "text"),
            spec("depends_on", "list"),
            spec("components", "list"),
            spec("paths", "list"),
        ]),
        ItemType::Doc => out.extend([spec("order", "number"), spec("components", "list")]),
        ItemType::Plan => out.push(spec("milestone", "ref")),
        ItemType::Milestone => out.push(spec("target", "date")),
        ItemType::Decision => out.push(spec("supersedes", "ref")),
        ItemType::Reference => out.extend([
            spec("resource", "url"),
            spec("library", "text"),
            spec("version", "text"),
        ]),
        ItemType::System | ItemType::Container | ItemType::Component => out.extend([
            spec("technology", "text"),
            spec("kind", "text"),
            spec("parent", "ref"),
            spec("code", "list"),
            spec("entry", "list"),
            spec("uses", "list"),
            spec("libraries", "list"),
        ]),
    }
    out.extend([spec("tags", "list"), spec("links", "list"), spec("stale_after", "date")]);
    out
}

pub fn api_config(repo: &Repo) -> ApiConfig {
    let config = &repo.store.config;
    ApiConfig {
        task_prefix: config.task_prefix.clone(),
        statuses: TYPES
            .iter()
            .map(|ty| (ty.as_str().to_string(), config.statuses_for(*ty)))
            .collect(),
        priorities: config.priorities.clone(),
        task_kinds: config.task_kinds.clone(),
        fields: TYPES
            .iter()
            .map(|ty| (ty.as_str().to_string(), fields_of(repo, *ty)))
            .collect(),
    }
}
