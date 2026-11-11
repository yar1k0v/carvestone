//! TypeScript and JavaScript extraction: classes, interfaces, enums, type
//! aliases, functions, methods, imports/exports and the reference sites a
//! resolver needs. tree-sitter recovers from broken input, so a parse error
//! costs the broken statement, not the file.
//!
//! Conventions the resolver reads back:
//! - qnames are `<path without extension>::Name` and `...::Class.member`;
//! - a top-level declaration that is exported has a signature starting with
//!   `export ` (`export default ` for the default export);
//! - import specifiers become [`ImportDecl`]s: a named or default binding has
//!   `path = "<spec>#<imported>"` and `alias = Some(local)`; a namespace
//!   binding has `glob = true` and `alias = Some(local)`; a side-effect import
//!   has no alias. `is_static` marks an export form: `export { a as b } from
//!   './x'` is `path = "./x#a"`, `alias = Some("b")`; a local `export { a }`
//!   has an empty spec (`path = "#a"`); `export * from` is `glob` without alias;
//! - an `Imports` site has the specifier as qualifier and the imported name
//!   (`default`, or `*` for a namespace or side-effect import);
//! - a member call has qualifier `.<receiver text>`; a `new` or plain call has
//!   no qualifier; base types and type references carry a dotted path.

use tree_sitter::{Node, Parser};

use crate::csharp::{collapse, line_of, scope_at, text};
use crate::facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning};

/// Deeper trees are truncated: a declaration is never worth a stack overflow.
const MAX_DEPTH: usize = 128;
/// A receiver longer than this is not a name path worth keeping.
const MAX_RECEIVER: usize = 120;

pub(crate) fn extract_ts(path: &str, src: &str) -> FileFacts {
    extract(Lang::TypeScript, path, src)
}

pub(crate) fn extract_js(path: &str, src: &str) -> FileFacts {
    extract(Lang::JavaScript, path, src)
}

struct Sink {
    decls: Vec<Decl>,
    refs: Vec<RefSite>,
    imports: Vec<ImportDecl>,
    warnings: Vec<Warning>,
    module: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Export {
    No,
    Named,
    Default,
}

fn extract(lang: Lang, path: &str, src: &str) -> FileFacts {
    let mut facts = FileFacts::new(lang, path);
    let mut parser = Parser::new();
    let language: tree_sitter::Language = if lang == Lang::JavaScript {
        tree_sitter_javascript::LANGUAGE.into()
    } else if path.to_ascii_lowercase().ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };
    if parser.set_language(&language).is_err() {
        facts.warnings.push(Warning {
            message: "TypeScript grammar unavailable".to_string(),
            line: 1,
        });
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        facts.warnings.push(Warning {
            message: "TypeScript parser returned no tree".to_string(),
            line: 1,
        });
        return facts;
    };
    let root = tree.root_node();

    let mut sink = Sink {
        decls: vec![Decl {
            name: file_stem(path),
            qname: path.to_string(),
            kind: DeclKind::File,
            parent: None,
            signature: String::new(),
            doc: None,
            line_start: 1,
            line_end: src.lines().count().max(1) as u32,
        }],
        refs: Vec::new(),
        imports: Vec::new(),
        warnings: Vec::new(),
        module: module_of(path),
    };
    walk_decls(root, src, 0, 0, &mut sink);
    walk_sites(root, src, &mut sink, 0);

    facts.decls = sink.decls;
    facts.refs = sink.refs;
    facts.imports = sink.imports;
    let mut warnings = sink.warnings;
    warnings.sort_by_key(|warning| warning.line);
    warnings.dedup_by_key(|warning| warning.line);
    facts.warnings.extend(warnings);
    facts
}

// ---------------------------------------------------------------- declarations

fn walk_decls(node: Node, src: &str, parent: usize, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        decl_of(child, child, src, parent, Export::No, depth, out);
    }
}

fn decl_of(node: Node, outer: Node, src: &str, parent: usize, export: Export, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    match node.kind() {
        "export_statement" => {
            let default = (0..node.child_count()).any(|i| node.child(i as u32).is_some_and(|c| c.kind() == "default"));
            let export = if default { Export::Default } else { Export::Named };
            if let Some(declaration) = node.child_by_field_name("declaration") {
                decl_of(declaration, node, src, parent, export, depth + 1, out);
            } else if default
                && let Some(value) = node.child_by_field_name("value")
                && matches!(
                    value.kind(),
                    "function_expression" | "arrow_function" | "class" | "generator_function"
                )
            {
                let kind = if value.kind() == "class" {
                    DeclKind::Type
                } else {
                    DeclKind::Function
                };
                let header = header_text(value, src);
                let index = push(out, node, node, src, "default", kind, parent, sig(export, &header));
                if kind == DeclKind::Type
                    && let Some(body) = value.child_by_field_name("body")
                {
                    walk_members(body, src, index, depth + 1, out);
                }
            }
        }
        "ambient_declaration" | "expression_statement" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                if matches!(
                    child.kind(),
                    "internal_module"
                        | "module"
                        | "class_declaration"
                        | "abstract_class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "type_alias_declaration"
                        | "function_declaration"
                        | "function_signature"
                        | "generator_function_declaration"
                        | "lexical_declaration"
                        | "variable_declaration"
                ) {
                    decl_of(child, outer, src, parent, export, depth + 1, out);
                }
            }
        }
        "class_declaration" | "abstract_class_declaration" | "class" => {
            let name = name_or_default(node, src, export);
            if name.is_empty() {
                return;
            }
            let index = push(
                out,
                node,
                outer,
                src,
                &name,
                DeclKind::Type,
                parent,
                sig(export, &header_text(node, src)),
            );
            if let Some(body) = node.child_by_field_name("body") {
                walk_members(body, src, index, depth + 1, out);
            }
        }
        "interface_declaration" | "enum_declaration" | "type_alias_declaration" => {
            let name = name_or_default(node, src, export);
            if name.is_empty() {
                return;
            }
            let index = push(
                out,
                node,
                outer,
                src,
                &name,
                DeclKind::Type,
                parent,
                sig(export, &header_text(node, src)),
            );
            if let Some(body) = node.child_by_field_name("body") {
                walk_members(body, src, index, depth + 1, out);
            }
        }
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            let name = name_or_default(node, src, export);
            if name.is_empty() {
                return;
            }
            push(
                out,
                node,
                outer,
                src,
                &name,
                DeclKind::Function,
                parent,
                sig(export, &header_text(node, src)),
            );
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            let declarators: Vec<Node> = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "variable_declarator")
                .collect();
            for declarator in declarators {
                let Some(name_node) = declarator.child_by_field_name("name") else {
                    continue;
                };
                if name_node.kind() != "identifier" {
                    continue;
                }
                let name = text(name_node, src).trim().to_string();
                let value = declarator.child_by_field_name("value");
                // `const x = require('y')` is an import binding, not a declaration.
                if value.is_some_and(|v| {
                    v.kind() == "call_expression"
                        && v.child_by_field_name("function")
                            .is_some_and(|f| f.kind() == "identifier" && text(f, src) == "require")
                }) {
                    continue;
                }
                let function = value.filter(|v| {
                    matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function" | "generator_function"
                    )
                });
                let kind = if function.is_some() {
                    DeclKind::Function
                } else {
                    DeclKind::Member
                };
                let end = function
                    .and_then(|f| f.child_by_field_name("body"))
                    .map_or_else(|| value.map_or(node.end_byte(), |v| v.start_byte()), |b| b.start_byte());
                let header = collapse(src.get(node.start_byte()..end).unwrap_or(""));
                let header = header.trim().trim_end_matches([';', '=']).trim().to_string();
                let index = push(out, node, outer, src, &name, kind, parent, sig(export, &header));
                if let Some(object) = value.and_then(object_literal) {
                    walk_object(object, src, index, depth + 1, out);
                }
            }
        }
        "internal_module" | "module" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let name = text(name_node, src).trim().trim_matches(['"', '\'']).to_string();
            if name.is_empty() {
                return;
            }
            let index = push(
                out,
                node,
                outer,
                src,
                &name,
                DeclKind::Module,
                parent,
                sig(export, &header_text(node, src)),
            );
            if let Some(body) = node.child_by_field_name("body") {
                walk_decls(body, src, index, depth + 1, out);
            }
        }
        _ => {}
    }
}

fn walk_members(body: Node, src: &str, owner: usize, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut cursor = body.walk();
    let children: Vec<Node> = body.named_children(&mut cursor).collect();
    for member in children {
        let kind = member.kind();
        match kind {
            "method_definition" | "abstract_method_signature" | "method_signature" => {
                let Some(name) = member_name(member, src) else { continue };
                push(
                    out,
                    member,
                    member,
                    src,
                    &name,
                    DeclKind::Method,
                    owner,
                    header_text(member, src),
                );
            }
            "public_field_definition" | "field_definition" | "property_signature" => {
                let Some(name) = member_name(member, src) else { continue };
                let value = member.child_by_field_name("value");
                let function = value.filter(|v| {
                    matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function" | "generator_function"
                    )
                });
                let decl_kind = if function.is_some() {
                    DeclKind::Method
                } else {
                    DeclKind::Member
                };
                let end = function.and_then(|f| f.child_by_field_name("body")).map_or_else(
                    || value.map_or(member.end_byte(), |v| v.start_byte()),
                    |b| b.start_byte(),
                );
                let header = collapse(src.get(member.start_byte()..end).unwrap_or(""));
                let header = header.trim().trim_end_matches([';', '=', ',']).trim().to_string();
                push(out, member, member, src, &name, decl_kind, owner, header);
            }
            "enum_assignment" | "property_identifier" => {
                let name_node = member.child_by_field_name("name").unwrap_or(member);
                let name = text(name_node, src).trim().trim_matches(['"', '\'']).to_string();
                if !name.is_empty() {
                    push(
                        out,
                        member,
                        member,
                        src,
                        &name,
                        DeclKind::Member,
                        owner,
                        collapse(text(member, src)),
                    );
                }
            }
            _ => {}
        }
    }
}

/// The object a value is, through `as`, `satisfies` and parentheses: `const api = { ... } as const`.
fn object_literal(value: Node) -> Option<Node> {
    match value.kind() {
        "object" => Some(value),
        "as_expression" | "satisfies_expression" | "parenthesized_expression" => {
            value.named_child(0).and_then(object_literal)
        }
        _ => None,
    }
}

/// The methods of an object literal: `{ run() {} }` and `{ run: async () => {} }`. Plain values are
/// left out; an object's methods are what a reader looks for.
fn walk_object(object: Node, src: &str, owner: usize, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut cursor = object.walk();
    let children: Vec<Node> = object.named_children(&mut cursor).collect();
    for entry in children {
        match entry.kind() {
            "method_definition" => {
                let Some(name) = member_name(entry, src) else { continue };
                push(
                    out,
                    entry,
                    entry,
                    src,
                    &name,
                    DeclKind::Method,
                    owner,
                    header_text(entry, src),
                );
            }
            "pair" => {
                let Some(key) = entry.child_by_field_name("key") else {
                    continue;
                };
                if key.kind() == "computed_property_name" {
                    continue;
                }
                let Some(function) = entry.child_by_field_name("value").filter(|v| {
                    matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function" | "generator_function"
                    )
                }) else {
                    continue;
                };
                let name = text(key, src).trim().trim_matches(['"', '\'']).to_string();
                if name.is_empty() {
                    continue;
                }
                let end = function
                    .child_by_field_name("body")
                    .map_or(function.end_byte(), |b| b.start_byte());
                let header = collapse(src.get(entry.start_byte()..end).unwrap_or(""));
                let header = header.trim().trim_end_matches("=>").trim().to_string();
                push(out, entry, entry, src, &name, DeclKind::Method, owner, header);
            }
            _ => {}
        }
    }
}

fn member_name(member: Node, src: &str) -> Option<String> {
    let node = member
        .child_by_field_name("name")
        .or_else(|| member.child_by_field_name("property"))?;
    if node.kind() == "computed_property_name" {
        return None;
    }
    let name = text(node, src).trim().trim_matches(['"', '\'']).to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn name_or_default(node: Node, src: &str, export: Export) -> String {
    let name = node
        .child_by_field_name("name")
        .map(|n| text(n, src).trim().to_string())
        .unwrap_or_default();
    if name.is_empty() && export == Export::Default {
        "default".to_string()
    } else {
        name
    }
}

fn sig(export: Export, header: &str) -> String {
    match export {
        Export::No => header.to_string(),
        Export::Named => format!("export {header}"),
        Export::Default => format!("export default {header}"),
    }
}

/// Add a declaration; a repeat of the same qualified name (an overload or a
/// signature plus its implementation) widens the first one.
#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Sink,
    node: Node,
    outer: Node,
    src: &str,
    name: &str,
    kind: DeclKind,
    parent: usize,
    signature: String,
) -> usize {
    let qname = if parent == 0 {
        format!("{}::{name}", out.module)
    } else {
        format!("{}.{name}", out.decls[parent].qname)
    };
    let start = line_of(outer.start_position().row);
    let end = line_of(node.end_position().row).max(line_of(outer.end_position().row));
    if let Some(existing) = out.decls.iter().rposition(|d| d.qname == qname && d.kind == kind) {
        let decl = &mut out.decls[existing];
        decl.line_start = decl.line_start.min(start);
        decl.line_end = decl.line_end.max(end);
        return existing;
    }
    out.decls.push(Decl {
        name: name.to_string(),
        qname,
        kind,
        parent: Some(parent),
        signature,
        doc: doc_above(outer, src),
        line_start: start,
        line_end: end,
    });
    out.decls.len() - 1
}

/// The declaration header: everything before the body, whitespace collapsed.
fn header_text(node: Node, src: &str) -> String {
    let start = node.start_byte();
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    if end <= start {
        return String::new();
    }
    let header = collapse(src.get(start..end).unwrap_or(""));
    header.trim().trim_end_matches(';').trim().to_string()
}

/// The `/** */` block or `//` lines right above `node`.
fn doc_above(node: Node, src: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut expected = node.start_position().row;
    let mut current = node;
    while let Some(prev) = current.prev_sibling() {
        if prev.kind() != "comment" || prev.end_position().row + 1 < expected {
            break;
        }
        let raw = text(prev, src).trim();
        if let Some(body) = raw.strip_prefix("/*") {
            let body = body.strip_suffix("*/").unwrap_or(body);
            let block: Vec<String> = body
                .lines()
                .map(|line| line.trim().trim_start_matches('*').trim().to_string())
                .collect();
            lines.extend(block.into_iter().rev());
        } else if let Some(body) = raw.strip_prefix("//") {
            lines.push(body.trim().to_string());
        } else {
            break;
        }
        expected = prev.start_position().row;
        current = prev;
    }
    lines.reverse();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

// ---------------------------------------------------------------- sites

fn walk_sites(node: Node, src: &str, out: &mut Sink, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if node.is_error() || node.is_missing() {
        out.warnings.push(Warning {
            message: "syntax error".to_string(),
            line: line_of(node.start_position().row),
        });
    }
    match node.kind() {
        "import_statement" => {
            import_statement(node, src, out);
            return;
        }
        "export_statement" => export_statement(node, src, out),
        "variable_declarator" => {
            if require_binding(node, src, out) {
                return;
            }
        }
        "call_expression" => {
            if call_site(node, src, out, depth) {
                return;
            }
        }
        "new_expression" => {
            if let Some(constructor) = node.child_by_field_name("constructor")
                && let Some((name, receiver)) = callee_parts(constructor, src)
            {
                let qualifier = receiver.map(|r| format!(".{r}"));
                push_site(out, name, qualifier, RefKind::Calls, node);
            }
        }
        "extends_clause" | "extends_type_clause" => {
            heritage(node, src, RefKind::Inherits, out);
            return;
        }
        "implements_clause" => {
            heritage(node, src, RefKind::Implements, out);
            return;
        }
        "class_heritage" => {
            // JavaScript: `extends Base` without a clause node.
            let mut cursor = node.walk();
            let has_clause = node
                .named_children(&mut cursor)
                .any(|c| matches!(c.kind(), "extends_clause" | "implements_clause"));
            if !has_clause {
                heritage(node, src, RefKind::Inherits, out);
                return;
            }
        }
        "decorator" => {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            if let Some(expression) = first
                && matches!(expression.kind(), "identifier" | "member_expression")
                && let Some((name, receiver)) = callee_parts(expression, src)
            {
                push_site(out, name, receiver.map(|r| format!(".{r}")), RefKind::Calls, node);
            }
        }
        "type_identifier" => {
            let is_name = node
                .parent()
                .and_then(|p| p.child_by_field_name("name"))
                .is_some_and(|n| n.id() == node.id());
            if !is_name {
                let name = text(node, src).trim().to_string();
                if !name.is_empty() {
                    push_site(out, name, None, RefKind::References, node);
                }
            }
            return;
        }
        "nested_type_identifier" => {
            if let Some((name, qualifier)) = type_parts(node, src) {
                push_site(out, name, qualifier, RefKind::References, node);
            }
            return;
        }
        "jsx_opening_element" | "jsx_self_closing_element" => {
            if let Some(tag) = node.child_by_field_name("name")
                && let Some((name, qualifier)) = type_parts(tag, src)
                && name.starts_with(|c: char| c.is_uppercase())
            {
                push_site(out, name, qualifier, RefKind::References, node);
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in children {
        walk_sites(child, src, out, depth + 1);
    }
}

fn push_site(out: &mut Sink, name: String, qualifier: Option<String>, kind: RefKind, node: Node) {
    let line = line_of(node.start_position().row);
    let scope = scope_at(&out.decls, line);
    out.refs.push(RefSite {
        name,
        qualifier,
        kind,
        line,
        scope,
    });
}

/// A call. Returns true when the whole node was handled here.
fn call_site(node: Node, src: &str, out: &mut Sink, depth: usize) -> bool {
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() == "identifier" && text(function, src) == "require" {
        if let Some(spec) = first_string_argument(node, src) {
            side_effect_import(out, &spec, node);
        }
        return true;
    }
    if let Some((name, receiver)) = callee_parts(function, src) {
        // On the line of the name, where a chained call is reported.
        let at = function.child_by_field_name("property").unwrap_or(node);
        push_site(out, name, receiver.map(|r| format!(".{r}")), RefKind::Calls, at);
    }
    // Calls inside the callee and the arguments still count.
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in children {
        if child.id() == function.id() {
            if function.kind() == "member_expression"
                && let Some(object) = function.child_by_field_name("object")
            {
                walk_sites(object, src, out, depth + 1);
            } else if !matches!(function.kind(), "identifier" | "member_expression" | "super" | "import") {
                walk_sites(function, src, out, depth + 1);
            }
            continue;
        }
        walk_sites(child, src, out, depth + 1);
    }
    true
}

/// `(name, receiver text)` of a callee: `f`, `a.b.f`, `this.f`.
fn callee_parts(function: Node, src: &str) -> Option<(String, Option<String>)> {
    match function.kind() {
        "identifier" => {
            let name = text(function, src).trim().to_string();
            if name.is_empty() { None } else { Some((name, None)) }
        }
        "member_expression" => {
            let property = function.child_by_field_name("property")?;
            let object = function.child_by_field_name("object")?;
            let name = text(property, src).trim().to_string();
            let receiver = collapse(text(object, src));
            if name.is_empty() {
                return None;
            }
            let receiver = if receiver.len() > MAX_RECEIVER {
                String::new()
            } else {
                receiver
            };
            Some((name, Some(receiver)))
        }
        "parenthesized_expression" | "non_null_expression" | "as_expression" => {
            let mut cursor = function.walk();
            let inner = function.named_children(&mut cursor).next()?;
            callee_parts(inner, src)
        }
        _ => None,
    }
}

/// `(name, dotted qualifier)` of a type or heritage name.
fn type_parts(node: Node, src: &str) -> Option<(String, Option<String>)> {
    match node.kind() {
        "identifier" | "type_identifier" | "property_identifier" => {
            let name = text(node, src).trim().to_string();
            if name.is_empty() { None } else { Some((name, None)) }
        }
        "nested_type_identifier" | "nested_identifier" => {
            let name = text(node.child_by_field_name("name")?, src).trim().to_string();
            let module = node
                .child_by_field_name("module")
                .or_else(|| node.child_by_field_name("object"))?;
            let qualifier = collapse(text(module, src));
            Some((name, if qualifier.is_empty() { None } else { Some(qualifier) }))
        }
        "member_expression" => {
            let name = text(node.child_by_field_name("property")?, src).trim().to_string();
            let qualifier = collapse(text(node.child_by_field_name("object")?, src));
            Some((name, if qualifier.is_empty() { None } else { Some(qualifier) }))
        }
        "jsx_namespace_name" => None,
        "generic_type" => type_parts(node.child_by_field_name("name")?, src),
        "call_expression" => type_parts(node.child_by_field_name("function")?, src),
        _ => None,
    }
}

fn heritage(node: Node, src: &str, kind: RefKind, out: &mut Sink) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        if matches!(child.kind(), "type_arguments" | "comment") {
            continue;
        }
        if let Some((name, qualifier)) = type_parts(child, src) {
            push_site(out, name, qualifier, kind, child);
        }
    }
}

// ---------------------------------------------------------------- imports

fn import_statement(node: Node, src: &str, out: &mut Sink) {
    let line = line_of(node.start_position().row);
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    let source = node.child_by_field_name("source").map(|s| string_value(s, src));
    let mut handled = false;
    for child in &children {
        match child.kind() {
            "import_clause" => {
                let Some(spec) = &source else { continue };
                handled = true;
                let mut inner_cursor = child.walk();
                let inner: Vec<Node> = child.named_children(&mut inner_cursor).collect();
                for item in inner {
                    match item.kind() {
                        "identifier" => {
                            let local = text(item, src).trim().to_string();
                            named_import(out, spec, "default", &local, line, false);
                        }
                        "namespace_import" => {
                            let mut c = item.walk();
                            let local = item
                                .named_children(&mut c)
                                .find(|n| n.kind() == "identifier")
                                .map(|n| text(n, src).trim().to_string());
                            if let Some(local) = local {
                                namespace_import(out, spec, Some(&local), line, false);
                            }
                        }
                        "named_imports" => named_specifiers(item, src, spec, line, false, out),
                        _ => {}
                    }
                }
            }
            "import_require_clause" => {
                // `import x = require('y')`
                let mut c = child.walk();
                let parts: Vec<Node> = child.named_children(&mut c).collect();
                let local = parts.iter().find(|n| n.kind() == "identifier");
                let spec = parts.iter().find(|n| n.kind() == "string");
                if let (Some(local), Some(spec)) = (local, spec) {
                    let spec = string_value(*spec, src);
                    namespace_import(out, &spec, Some(text(*local, src).trim()), line, false);
                    handled = true;
                }
            }
            _ => {}
        }
    }
    if !handled && let Some(spec) = source {
        side_effect_import(out, &spec, node);
    }
}

fn named_specifiers(list: Node, src: &str, spec: &str, line: u32, export: bool, out: &mut Sink) {
    let mut cursor = list.walk();
    let items: Vec<Node> = list.named_children(&mut cursor).collect();
    for item in items {
        if !matches!(item.kind(), "import_specifier" | "export_specifier") {
            continue;
        }
        let Some(name) = item.child_by_field_name("name") else {
            continue;
        };
        let imported = text(name, src).trim().trim_matches(['"', '\'']).to_string();
        let local = item
            .child_by_field_name("alias")
            .map_or_else(|| imported.clone(), |a| text(a, src).trim().to_string());
        if imported.is_empty() || local.is_empty() {
            continue;
        }
        let item_line = line_of(item.start_position().row).max(line);
        named_import(out, spec, &imported, &local, item_line, export);
    }
}

fn named_import(out: &mut Sink, spec: &str, imported: &str, local: &str, line: u32, export: bool) {
    out.imports.push(ImportDecl {
        path: format!("{spec}#{imported}"),
        alias: Some(local.to_string()),
        glob: false,
        is_static: export,
        global: false,
        line,
    });
    if !spec.is_empty() {
        out.refs.push(RefSite {
            name: imported.to_string(),
            qualifier: Some(spec.to_string()),
            kind: RefKind::Imports,
            line,
            scope: Some(0),
        });
    }
}

fn namespace_import(out: &mut Sink, spec: &str, local: Option<&str>, line: u32, export: bool) {
    out.imports.push(ImportDecl {
        path: spec.to_string(),
        alias: local.map(str::to_string),
        glob: true,
        is_static: export,
        global: false,
        line,
    });
    out.refs.push(RefSite {
        name: "*".to_string(),
        qualifier: Some(spec.to_string()),
        kind: RefKind::Imports,
        line,
        scope: Some(0),
    });
}

fn side_effect_import(out: &mut Sink, spec: &str, node: Node) {
    let line = line_of(node.start_position().row);
    out.imports.push(ImportDecl {
        path: spec.to_string(),
        alias: None,
        glob: false,
        is_static: false,
        global: false,
        line,
    });
    out.refs.push(RefSite {
        name: "*".to_string(),
        qualifier: Some(spec.to_string()),
        kind: RefKind::Imports,
        line,
        scope: Some(0),
    });
}

/// `const x = require('y')` and `const { a, b: c } = require('y')`.
fn require_binding(node: Node, src: &str, out: &mut Sink) -> bool {
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(function) = value.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "identifier" || text(function, src) != "require" {
        return false;
    }
    let Some(spec) = first_string_argument(value, src) else {
        return false;
    };
    let line = line_of(node.start_position().row);
    let Some(name) = node.child_by_field_name("name") else {
        return false;
    };
    match name.kind() {
        "identifier" => {
            namespace_import(out, &spec, Some(text(name, src).trim()), line, false);
        }
        "object_pattern" => {
            let mut cursor = name.walk();
            let items: Vec<Node> = name.named_children(&mut cursor).collect();
            let mut any = false;
            for item in items {
                match item.kind() {
                    "shorthand_property_identifier_pattern" => {
                        let local = text(item, src).trim().to_string();
                        named_import(out, &spec, &local, &local, line, false);
                        any = true;
                    }
                    "pair_pattern" => {
                        let key = item.child_by_field_name("key").map(|k| text(k, src).trim().to_string());
                        let local = item
                            .child_by_field_name("value")
                            .map(|v| text(v, src).trim().to_string());
                        if let (Some(key), Some(local)) = (key, local)
                            && !key.is_empty()
                            && !local.is_empty()
                        {
                            named_import(out, &spec, &key, &local, line, false);
                            any = true;
                        }
                    }
                    _ => {}
                }
            }
            if !any {
                side_effect_import(out, &spec, node);
            }
        }
        _ => side_effect_import(out, &spec, node),
    }
    true
}

fn export_statement(node: Node, src: &str, out: &mut Sink) {
    let line = line_of(node.start_position().row);
    let source = node.child_by_field_name("source").map(|s| string_value(s, src));
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in &children {
        match child.kind() {
            "export_clause" => {
                named_specifiers(*child, src, source.as_deref().unwrap_or(""), line, true, out);
            }
            "namespace_export" => {
                if let Some(spec) = &source {
                    let mut c = child.walk();
                    let local = child
                        .named_children(&mut c)
                        .find(|n| matches!(n.kind(), "identifier" | "string"))
                        .map(|n| text(n, src).trim().trim_matches(['"', '\'']).to_string());
                    namespace_import(out, spec, local.as_deref(), line, true);
                }
            }
            "*" => {
                // `export * from './x'`: the star token sits directly in the statement.
                if let Some(spec) = &source
                    && !children.iter().any(|c| c.kind() == "namespace_export")
                {
                    namespace_import(out, spec, None, line, true);
                }
            }
            _ => {}
        }
    }
    // `export default Foo;`
    if let Some(value) = node.child_by_field_name("value")
        && value.kind() == "identifier"
    {
        let local = text(value, src).trim().to_string();
        out.imports.push(ImportDecl {
            path: format!("#{local}"),
            alias: Some("default".to_string()),
            glob: false,
            is_static: true,
            global: false,
            line,
        });
    }
}

fn first_string_argument(call: Node, src: &str) -> Option<String> {
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    if matches!(first.kind(), "string" | "template_string") {
        let value = string_value(first, src);
        if value.is_empty() { None } else { Some(value) }
    } else {
        None
    }
}

fn string_value(node: Node, src: &str) -> String {
    text(node, src).trim().trim_matches(['"', '\'', '`']).to_string()
}

// ---------------------------------------------------------------- paths

/// `src/web/main.ts` becomes `src/web/main`.
fn module_of(path: &str) -> String {
    let normal = path.replace('\\', "/");
    match normal.rsplit_once('.') {
        Some((stem, ext)) if !ext.contains('/') => stem.to_string(),
        _ => normal,
    }
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
