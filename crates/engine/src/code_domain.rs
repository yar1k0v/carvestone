//! `GET /api/code/domain`: the entities of the code, their key fields and how they relate.
//!
//! A type is an entity when it derives from a base of the usual entity names, or when it sits in
//! a folder named for entities. Relations come from its fields: a
//! field of an entity type is `has one`, a collection of one is `has many`, and a field `CustomerId`
//! next to an entity `Customer` is `refers to`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::code::open;
use crate::code_index::building_note;
use crate::code_map::{CodeMap, Scope, Sym, code_map};
use crate::repo::{EngineError, Repo};

/// Fields listed per entity: ids first, then in source order.
const MAX_FIELDS: usize = 12;
/// Base types that make a type an entity. They are not entities themselves.
const ENTITY_BASES: [&str; 6] = [
    "Entity",
    "BaseEntity",
    "EntityBase",
    "AggregateRoot",
    "IAggregateRoot",
    "IEntity",
];
/// Folders whose types are entities, in any language.
const ENTITY_FOLDERS: [&str; 2] = ["entities", "entity"];
/// More folders that hold entities outside C#, where a model folder is rarely request data.
const MODEL_FOLDERS: [&str; 2] = ["models", "domain"];
/// Generic types that hold many of what they name.
const COLLECTIONS: [&str; 12] = [
    "ICollection",
    "IList",
    "IEnumerable",
    "IReadOnlyList",
    "IReadOnlyCollection",
    "List",
    "HashSet",
    "ISet",
    "Set",
    "Vec",
    "Array",
    "Iterable",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntityField {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(rename = "type", optional))]
    pub type_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Entity {
    pub name: String,
    /// The qualified name, to link to the class page.
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub line: Option<u32>,
    pub fields: Vec<EntityField>,
    /// Fields the list left out.
    #[serde(skip_serializing_if = "crate::code::is_zero")]
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>", optional))]
    pub more_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntityRelation {
    pub from: String,
    pub to: String,
    /// `has one`, `has many`, `refers to` or `is a`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<String>,
}

/// What `GET /api/code/domain` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DomainModel {
    pub entities: Vec<Entity>,
    pub relations: Vec<EntityRelation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
}

/// A field as a header states it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    name: String,
    type_name: Option<String>,
}

/// The type a field header states. C# and Dart write it before the name, TypeScript and Rust after
/// a colon.
fn field_type(lang: &str, name: &str, signature: &str) -> Option<String> {
    let header = signature.split(['=', ';']).next().unwrap_or(signature).trim();
    if let Some((_, after)) = header
        .split_once(':')
        .filter(|_| matches!(lang, "typescript" | "javascript" | "rust"))
    {
        return Some(after.trim().to_string()).filter(|t| !t.is_empty());
    }
    let before = header.strip_suffix(name)?.trim();
    let mut depth = 0i32;
    let mut start = 0;
    for (at, c) in before.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ' ' if depth == 0 => start = at + 1,
            _ => {}
        }
    }
    Some(before[start..].to_string()).filter(|t| !t.is_empty())
}

/// The fields of a type, in source order: properties and instance fields, not methods or constants.
fn fields_of(map: &CodeMap, owner: &Sym) -> Vec<Field> {
    let mut members: Vec<&Sym> = map.inside(owner.id).collect();
    members.sort_by_key(|member| member.line_start);
    let mut fields = Vec::new();
    for member in members {
        let property = owner.lang == "csharp" && member.kind == "method" && !member.signature.contains('(');
        let field = member.kind == "member" && owner.lang != "csharp";
        if !(property || field) || member.has_word("static") || member.has_word("const") {
            continue;
        }
        fields.push(Field {
            name: member.name.clone(),
            type_name: field_type(&owner.lang, &member.name, &member.signature),
        });
    }
    fields
}

/// The fields a type takes from its base types of the project, nearest last.
fn inherited_fields(map: &CodeMap, owner: &Sym, depth: usize) -> Vec<Field> {
    if depth > 4 {
        return Vec::new();
    }
    let mut ends = vec![owner.id];
    ends.extend(map.inside(owner.id).map(|member| member.id));
    let mut seen = BTreeSet::new();
    let mut fields = Vec::new();
    for end in ends {
        for link in map.out_of(end).iter().filter(|link| link.kind == "inherits") {
            if let Some(base) = map.owner_type(link.other)
                && base.id != owner.id
                && !base.has_word("interface")
                && seen.insert(base.id)
            {
                fields.extend(inherited_fields(map, base, depth + 1));
                fields.extend(fields_of(map, base));
            }
        }
    }
    fields
}

/// The bare type a field holds, and whether it holds many.
fn held_type(type_name: &str) -> (String, bool) {
    let clean = type_name.trim().trim_end_matches(['?', '!']).trim();
    if let Some(element) = clean.strip_suffix("[]") {
        return (held_type(element).0, true);
    }
    let Some((head, rest)) = clean.split_once('<') else {
        return (clean.rsplit(['.', ':']).next().unwrap_or(clean).to_string(), false);
    };
    let head = head.rsplit(['.', ':']).next().unwrap_or(head);
    let inner = rest.rsplit_once('>').map_or(rest, |(inner, _)| inner);
    if COLLECTIONS.contains(&head) {
        return (held_type(inner).0, true);
    }
    if matches!(head, "Option" | "Box" | "Arc" | "Rc") {
        return held_type(inner);
    }
    (head.to_string(), false)
}

/// The base type names a header states, without generics: `class A : B<T>, IC` gives `B` and `IC`.
fn bases(map: &CodeMap, sym: &Sym) -> Vec<String> {
    let mut found = BTreeSet::new();
    let mut ends = vec![sym.id];
    ends.extend(map.inside(sym.id).map(|member| member.id));
    for end in ends {
        for link in map.out_of(end) {
            if matches!(link.kind.as_str(), "inherits" | "implements")
                && let Some(base) = map.owner_type(link.other)
                && base.id != sym.id
            {
                found.insert(base.name.clone());
            }
        }
    }
    if let Some((_, list)) = sym.signature.rsplit_once(" : ") {
        for base in list.split(',') {
            let base = base.split('<').next().unwrap_or(base).trim();
            let base = base.rsplit('.').next().unwrap_or(base);
            if !base.is_empty() {
                found.insert(base.to_string());
            }
        }
    }
    found.into_iter().collect()
}

/// The folder words of a path, lower-cased, without the file.
fn folders(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = path.split('/').map(str::to_ascii_lowercase).collect();
    parts.pop();
    parts
}

/// Whether the type is a concrete data type at all.
fn concrete(sym: &Sym) -> bool {
    sym.kind == "type"
        && !sym.has_word("interface")
        && !sym.has_word("trait")
        && !sym.has_word("abstract")
        && !sym.has_word("enum")
        && !sym.has_word("static")
}

/// True when something other than where a type sits says it is an entity.
fn entity_by_base(map: &CodeMap, sym: &Sym) -> bool {
    bases(map, sym).iter().any(|base| ENTITY_BASES.contains(&base.as_str()))
}

fn entity_by_folder(map: &CodeMap, sym: &Sym) -> bool {
    let parts = folders(map.path(sym));
    let holds = |names: &[&str]| parts.iter().any(|part| names.contains(&part.as_str()));
    holds(&ENTITY_FOLDERS) || (sym.lang != "csharp" && holds(&MODEL_FOLDERS))
}

/// The entities of the code; `part` narrows them to one architecture part.
pub fn domain(repo: &Repo, part: Option<&str>) -> Result<DomainModel, EngineError> {
    let index = open(repo)?;
    let conn = index.conn();
    let snapshot = repo.snapshot()?;
    let scope = Scope::new(&snapshot, part)?;
    let map = code_map(repo, conn)?;

    let mut found: BTreeMap<String, &Sym> = BTreeMap::new();
    let mut syms: Vec<&Sym> = map
        .syms
        .values()
        .filter(|sym| sym.kind == "type" && !map.sym_is_test(sym))
        .collect();
    syms.sort_by_key(|sym| sym.id);
    for sym in syms {
        let is_entity = concrete(sym) && (entity_by_base(&map, sym) || entity_by_folder(&map, sym));
        if is_entity && scope.covers(map.path(sym)) && !ENTITY_BASES.contains(&sym.name.as_str()) {
            found.entry(sym.name.clone()).or_insert(sym);
        }
    }

    let mut entities = Vec::new();
    let mut relations: BTreeSet<(String, String, String)> = BTreeSet::new();
    for (name, sym) in &found {
        let mut fields = inherited_fields(&map, sym, 0);
        fields.extend(fields_of(&map, sym));
        let mut ids: Vec<&Field> = Vec::new();
        let mut others: Vec<&Field> = Vec::new();
        let linked: BTreeSet<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        for field in &fields {
            let held = field.type_name.as_deref().map(held_type);
            match held.as_ref().filter(|(target, _)| found.contains_key(target)) {
                Some((target, many)) => {
                    let label = if *many { "has many" } else { "has one" };
                    relations.insert((name.clone(), target.clone(), label.to_string()));
                }
                None => {
                    if field.name.eq_ignore_ascii_case("id") || field.name.ends_with("Id") {
                        ids.push(field);
                    } else {
                        others.push(field);
                    }
                }
            }
            // `CustomerId`, or `PrimaryCustomerId` when no entity is called `PrimaryCustomer`.
            if let Some(stem) = field.name.strip_suffix("Id").filter(|stem| !stem.is_empty())
                && !linked.contains(stem)
                && let Some(target) = target_of(stem, &found)
                && target != *name
            {
                relations.insert((name.clone(), target, "refers to".to_string()));
            }
        }
        for base in bases(&map, sym) {
            if found.contains_key(&base) && base != *name {
                relations.insert((name.clone(), base, "is a".to_string()));
            }
        }
        let all = ids.len() + others.len();
        let listed: Vec<EntityField> = ids
            .into_iter()
            .chain(others)
            .take(MAX_FIELDS)
            .map(|field| EntityField {
                name: field.name.clone(),
                type_name: field.type_name.clone(),
            })
            .collect();
        entities.push(Entity {
            name: name.clone(),
            symbol: sym.qname.clone(),
            file: Some(map.path(sym).to_string()),
            line: Some(sym.line_start),
            more_fields: all - listed.len(),
            fields: listed,
        });
    }
    Ok(DomainModel {
        entities,
        relations: relations
            .into_iter()
            .map(|(from, to, label)| EntityRelation {
                from,
                to,
                label: Some(label),
            })
            .collect(),
        note: building_note(conn)?,
    })
}

/// The entity a field stem names: itself, or the longest ending of it that is an entity.
fn target_of(stem: &str, found: &BTreeMap<String, &Sym>) -> Option<String> {
    if found.contains_key(stem) {
        return Some(stem.to_string());
    }
    stem.char_indices()
        .filter(|(at, c)| *at > 0 && c.is_uppercase())
        .map(|(at, _)| &stem[at..])
        .find(|tail| found.contains_key(*tail))
        .map(str::to_string)
}
