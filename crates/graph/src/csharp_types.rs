//! Declared types of variables. A typed local, parameter, field, property or
//! foreach variable becomes a `References` site: `name` and the type's own
//! qualifier name the type, and the site's `qualifier` is `"<variable>:<type
//! qualifier>"`. C# extraction emits no other `References` sites, so the
//! resolver reads every one of them as such a binding.

use tree_sitter::Node;

use crate::csharp::{collapse, line_of, scope_at, split_qualified, text};
use crate::facts::{Decl, DeclKind, RefKind, RefSite};

/// Types the language names with a keyword: never in the repository.
const PREDEFINED: &[&str] = &[
    "var", "dynamic", "object", "string", "bool", "byte", "sbyte", "char", "decimal", "double", "float", "int", "uint",
    "long", "ulong", "short", "ushort", "nint", "nuint", "void",
];

/// A variable and the type it was declared with.
pub(crate) struct Binding {
    pub type_qualifier: Option<String>,
    pub type_name: String,
}

impl Binding {
    /// The type as one dotted path.
    pub fn type_path(&self) -> String {
        match &self.type_qualifier {
            Some(qualifier) => format!("{qualifier}.{}", self.type_name),
            None => self.type_name.clone(),
        }
    }
}

/// The variable name of a binding site.
pub(crate) fn binding_var(site: &RefSite) -> Option<&str> {
    if site.kind != RefKind::References {
        return None;
    }
    site.qualifier.as_deref()?.split_once(':').map(|(var, _)| var)
}

/// The binding a `References` site encodes, or `None` for any other site.
pub(crate) fn parse_binding(site: &RefSite) -> Option<Binding> {
    if site.kind != RefKind::References {
        return None;
    }
    let (_, type_qualifier) = site.qualifier.as_deref()?.split_once(':')?;
    Some(Binding {
        type_qualifier: (!type_qualifier.is_empty()).then(|| type_qualifier.to_string()),
        type_name: site.name.clone(),
    })
}

/// Record the variables declared by `node` (a `parameter`, `variable_declaration`,
/// `property_declaration` or `foreach_statement`).
pub(crate) fn collect(node: Node, src: &str, decls: &[Decl], refs: &mut Vec<RefSite>) {
    match node.kind() {
        "parameter" => {
            if let (Some(type_node), Some(name)) = (node.child_by_field_name("type"), node.child_by_field_name("name"))
            {
                push(refs, decls, text(name, src), text(type_node, src), name, false);
            }
        }
        "property_declaration" => {
            if let (Some(type_node), Some(name)) = (node.child_by_field_name("type"), node.child_by_field_name("name"))
            {
                push(refs, decls, text(name, src), text(type_node, src), name, true);
            }
        }
        "foreach_statement" => {
            if let (Some(type_node), Some(name)) = (node.child_by_field_name("type"), node.child_by_field_name("left"))
            {
                push(refs, decls, text(name, src), text(type_node, src), name, false);
            }
        }
        "variable_declaration" => {
            let Some(type_node) = node.child_by_field_name("type") else {
                return;
            };
            let in_type = matches!(
                node.parent().map(|parent| parent.kind()),
                Some("field_declaration" | "event_field_declaration")
            );
            let declared = collapse(text(type_node, src));
            let mut cursor = node.walk();
            for declarator in node.named_children(&mut cursor) {
                if declarator.kind() != "variable_declarator" {
                    continue;
                }
                let Some(name) = declarator
                    .child_by_field_name("name")
                    .or_else(|| declarator.named_child(0).filter(|child| child.kind() == "identifier"))
                else {
                    continue;
                };
                let type_text = if declared == "var" {
                    created_type(declarator, src).unwrap_or_default()
                } else {
                    declared.clone()
                };
                push(refs, decls, text(name, src), &type_text, name, in_type);
            }
        }
        _ => {}
    }
}

/// The type of `new T(...)` when it initializes `declarator`.
fn created_type(declarator: Node, src: &str) -> Option<String> {
    let mut cursor = declarator.walk();
    for child in declarator.named_children(&mut cursor) {
        let expr = if child.kind() == "equals_value_clause" {
            child.named_child(0)?
        } else {
            child
        };
        if expr.kind() == "object_creation_expression" {
            return expr
                .child_by_field_name("type")
                .map(|type_node| collapse(text(type_node, src)));
        }
    }
    None
}

/// `member` is true for a field or property: its binding belongs to the type.
fn push(refs: &mut Vec<RefSite>, decls: &[Decl], var: &str, type_text: &str, at: Node, member: bool) {
    let Some((name, qualifier)) = normalize(type_text) else {
        return;
    };
    let var = var.trim();
    if var.is_empty() {
        return;
    }
    let line = line_of(at.start_position().row);
    let mut scope = scope_at(decls, line);
    if member {
        scope = scope
            .and_then(|index| decls.get(index))
            .and_then(|decl| if decl.kind == DeclKind::Type { None } else { decl.parent })
            .or(scope);
    }
    refs.push(RefSite {
        name,
        qualifier: Some(format!("{var}:{}", qualifier.unwrap_or_default())),
        kind: RefKind::References,
        line,
        scope,
    });
}

/// Name and qualifier of a type without generics, nullability or `global::`.
/// Arrays, tuples and keyword types have none.
fn normalize(type_text: &str) -> Option<(String, Option<String>)> {
    let mut text = type_text.trim();
    text = text.strip_prefix("global::").unwrap_or(text);
    while let Some(rest) = text.strip_suffix('?') {
        text = rest.trim_end();
    }
    if text.is_empty() || text.ends_with(']') || text.contains('(') || PREDEFINED.contains(&text) {
        return None;
    }
    let (name, qualifier) = split_qualified(text);
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some((name, qualifier))
}
