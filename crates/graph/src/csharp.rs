//! C# extraction: namespaces, types and the declarations inside them, plus
//! the reference sites (`calls`, `inherits`, `implements`) and using
//! directives a resolver needs. tree-sitter recovers from broken input, so a
//! parse error costs the broken declaration, not the file.

use tree_sitter::{Node, Parser};

use crate::facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning};

/// Deeper trees are truncated: a declaration is never worth a stack overflow.
const MAX_DEPTH: usize = 128;

const TYPE_KINDS: &[&str] = &[
    "class_declaration",
    "struct_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "record_struct_declaration",
    "delegate_declaration",
];

const METHOD_KINDS: &[&str] = &[
    "method_declaration",
    "constructor_declaration",
    "destructor_declaration",
    "operator_declaration",
    "conversion_operator_declaration",
    "property_declaration",
    "indexer_declaration",
    "event_declaration",
];

const MEMBER_KINDS: &[&str] = &[
    "field_declaration",
    "event_field_declaration",
    "enum_member_declaration",
];

/// Children that start the body; a signature stops where the first of them does.
const BODY_KINDS: &[&str] = &[
    "block",
    "declaration_list",
    "arrow_expression_clause",
    "accessor_list",
    "enum_member_declaration_list",
    "constructor_initializer",
];

pub(crate) fn extract_csharp(path: &str, src: &str) -> FileFacts {
    let mut facts = FileFacts::new(Lang::CSharp, path);
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).is_err() {
        facts.warnings.push(Warning {
            message: "C# grammar unavailable".to_string(),
            line: 1,
        });
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        facts.warnings.push(Warning {
            message: "C# parser returned no tree".to_string(),
            line: 1,
        });
        return facts;
    };
    let root = tree.root_node();

    facts.decls.push(Decl {
        name: file_stem(path),
        qname: path.to_string(),
        kind: DeclKind::File,
        parent: None,
        signature: String::new(),
        doc: None,
        line_start: 1,
        line_end: src.lines().count().max(1) as u32,
    });
    walk_decls(root, src, &[], Some(0), 0, &mut facts.decls);

    let mut refs = Vec::new();
    let mut imports = Vec::new();
    let mut warnings = Vec::new();
    walk_sites(root, src, &facts.decls, &mut refs, &mut imports, &mut warnings, 0);
    facts.refs = refs;
    facts.imports = imports;
    warnings.sort_by_key(|warning| warning.line);
    warnings.dedup_by_key(|warning| warning.line);
    facts.warnings.extend(warnings);
    facts
}

// ---------------------------------------------------------------- declarations

fn walk_decls(parent: Node, src: &str, ns: &[String], parent_decl: Option<usize>, depth: usize, decls: &mut Vec<Decl>) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut current_ns = ns.to_vec();
    let mut current_parent = parent_decl;
    let mut cursor = parent.walk();
    for child in parent.named_children(&mut cursor) {
        let kind = child.kind();
        if kind == "comment" {
            continue;
        }
        if kind == "namespace_declaration" {
            let name = decl_name(child, src);
            let qname = qualify(&current_ns, &name);
            let index = push(decls, child, src, &name, qname, DeclKind::Module, current_parent);
            let mut inner = current_ns.clone();
            inner.extend(name.split('.').map(str::to_string));
            walk_decls(child, src, &inner, Some(index), depth + 1, decls);
        } else if kind == "file_scoped_namespace_declaration" {
            let name = decl_name(child, src);
            let qname = qualify(&current_ns, &name);
            let index = push(decls, child, src, &name, qname, DeclKind::Module, current_parent);
            current_ns.extend(name.split('.').map(str::to_string));
            current_parent = Some(index);
        } else if TYPE_KINDS.contains(&kind) {
            let name = decl_name(child, src);
            let qname = qualify_in(decls, current_parent, &name);
            let index = push(decls, child, src, &name, qname, DeclKind::Type, current_parent);
            walk_decls(child, src, &current_ns, Some(index), depth + 1, decls);
        } else if METHOD_KINDS.contains(&kind) {
            let name = method_name(child, src);
            let qname = qualify_in(decls, current_parent, &name);
            push(decls, child, src, &name, qname, DeclKind::Method, current_parent);
        } else if MEMBER_KINDS.contains(&kind) {
            push_members(child, src, current_parent, decls);
        } else {
            walk_decls(child, src, &current_ns, current_parent, depth + 1, decls);
        }
    }
}

fn push(
    decls: &mut Vec<Decl>,
    node: Node,
    src: &str,
    name: &str,
    qname: String,
    kind: DeclKind,
    parent: Option<usize>,
) -> usize {
    decls.push(Decl {
        name: name.to_string(),
        qname,
        kind,
        parent,
        signature: header_text(node, src),
        doc: doc_above(node, src),
        line_start: line_of(node.start_position().row),
        line_end: line_of(node.end_position().row),
    });
    decls.len() - 1
}

/// A field or event declares one member per variable; an enum member one.
fn push_members(node: Node, src: &str, parent: Option<usize>, decls: &mut Vec<Decl>) {
    if node.kind() == "enum_member_declaration" {
        let name = decl_name(node, src);
        let qname = qualify_in(decls, parent, &name);
        decls.push(Decl {
            name,
            qname,
            kind: DeclKind::Member,
            parent,
            signature: header_text(node, src),
            doc: doc_above(node, src),
            line_start: line_of(node.start_position().row),
            line_end: line_of(node.end_position().row),
        });
        return;
    }
    let mut outer = node.walk();
    let Some(declaration) = node
        .named_children(&mut outer)
        .find(|child| child.kind() == "variable_declaration")
    else {
        return;
    };
    let type_text = declaration
        .child_by_field_name("type")
        .map(|node| collapse(text(node, src)))
        .unwrap_or_default();
    let mut cursor = declaration.walk();
    for variable in declaration.named_children(&mut cursor) {
        if variable.kind() != "variable_declarator" {
            continue;
        }
        let name = variable
            .child_by_field_name("name")
            .map(|node| text(node, src).trim().to_string());
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            continue;
        };
        let qname = qualify_in(decls, parent, &name);
        let signature = if type_text.is_empty() {
            name.clone()
        } else {
            format!("{type_text} {name}")
        };
        decls.push(Decl {
            name,
            qname,
            kind: DeclKind::Member,
            parent,
            signature,
            doc: doc_above(node, src),
            line_start: line_of(node.start_position().row),
            line_end: line_of(node.end_position().row),
        });
    }
}

fn decl_name(node: Node, src: &str) -> String {
    if let Some(field) = node.child_by_field_name("name") {
        let name = collapse(text(field, src));
        if !name.is_empty() {
            return name;
        }
    }
    let head = header_text(node, src);
    let before = head.split('(').next().unwrap_or(&head).trim();
    before.split_whitespace().next_back().unwrap_or("").to_string()
}

fn method_name(node: Node, src: &str) -> String {
    if node.kind() == "indexer_declaration" {
        return "this".to_string();
    }
    decl_name(node, src)
}

fn qualify(ns: &[String], name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{}.{name}", ns.join("."))
    }
}

fn qualify_in(decls: &[Decl], parent: Option<usize>, name: &str) -> String {
    match parent {
        Some(index) if decls[index].kind != DeclKind::File => format!("{}.{name}", decls[index].qname),
        _ => name.to_string(),
    }
}

/// The declaration header: attributes, comments and body removed, whitespace collapsed.
fn header_text(node: Node, src: &str) -> String {
    let mut start = node.start_byte();
    let mut end = node.end_byte();
    let mut comments = Vec::new();
    for index in 0..node.named_child_count() {
        let Some(child) = u32::try_from(index).ok().and_then(|i| node.named_child(i)) else {
            continue;
        };
        if child.kind() == "attribute_list" {
            start = start.max(child.end_byte());
        }
        if child.kind() == "comment" {
            comments.push(child.start_byte()..child.end_byte());
        }
        if BODY_KINDS.contains(&child.kind()) {
            end = end.min(child.start_byte());
        }
    }
    if end <= start {
        return String::new();
    }
    // A comment between the attributes and the name (often a commented-out attribute) is no part of it.
    let mut text = String::new();
    let mut at = start;
    for comment in comments.into_iter().filter(|c| c.start >= start && c.end <= end) {
        text.push_str(text_range(src, at..comment.start));
        text.push(' ');
        at = comment.end;
    }
    text.push_str(text_range(src, at..end));
    collapse(&text).trim_end_matches(';').trim().to_string()
}

/// The leading comment block immediately above `node`, markers removed. Only
/// a block that holds a doc comment (`///` or `/**`) counts as documentation.
fn doc_above(node: Node, src: &str) -> Option<String> {
    let mut blocks = Vec::new();
    let mut expected = node.start_position().row;
    let mut current = node;
    while let Some(prev) = current.prev_named_sibling() {
        if prev.kind() != "comment" || prev.end_position().row + 1 != expected {
            break;
        }
        blocks.push(text(prev, src).to_string());
        expected = prev.start_position().row;
        current = prev;
    }
    if blocks.is_empty() {
        return None;
    }
    blocks.reverse();
    strip_doc(&blocks)
}

fn strip_doc(blocks: &[String]) -> Option<String> {
    let mut lines = Vec::new();
    let mut is_doc = false;
    for raw in blocks {
        let raw = raw.trim();
        if let Some(body) = raw.strip_prefix("///") {
            is_doc = true;
            lines.push(body.trim().to_string());
        } else if let Some(body) = raw.strip_prefix("/**") {
            is_doc = true;
            lines.extend(block_lines(body));
        } else if let Some(body) = raw.strip_prefix("/*") {
            lines.extend(block_lines(body));
        } else if let Some(body) = raw.strip_prefix("//") {
            lines.push(body.trim().to_string());
        }
    }
    if !is_doc {
        return None;
    }
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

fn block_lines(body: &str) -> Vec<String> {
    let body = body.strip_suffix("*/").unwrap_or(body);
    body.lines()
        .map(|line| line.trim().trim_start_matches('*').trim().to_string())
        .collect()
}

// ---------------------------------------------------------------- sites

#[allow(clippy::too_many_arguments)]
fn walk_sites(
    node: Node,
    src: &str,
    decls: &[Decl],
    refs: &mut Vec<RefSite>,
    imports: &mut Vec<ImportDecl>,
    warnings: &mut Vec<Warning>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let kind = node.kind();
    if kind == "using_directive" {
        if let Some(import) = parse_using(node, src) {
            imports.push(import);
        }
        return;
    }
    if node.is_error() || node.is_missing() {
        warnings.push(Warning {
            message: "syntax error".to_string(),
            line: line_of(node.start_position().row),
        });
    }
    match kind {
        "invocation_expression" => {
            if let Some(site) = call_site(node, src, decls) {
                refs.push(site);
            }
        }
        "object_creation_expression" => {
            if let Some(site) = creation_site(node, src, decls) {
                refs.push(site);
            }
        }
        "base_list" => collect_bases(node, src, decls, refs),
        "parameter" | "variable_declaration" | "property_declaration" | "foreach_statement" => {
            crate::csharp_types::collect(node, src, decls, refs);
        }
        _ => {}
    }
    // All children, not just named ones: a missing `)` is an anonymous token.
    for index in 0..node.child_count() {
        if let Some(child) = u32::try_from(index).ok().and_then(|i| node.child(i)) {
            walk_sites(child, src, decls, refs, imports, warnings, depth + 1);
        }
    }
}

fn call_site(node: Node, src: &str, decls: &[Decl]) -> Option<RefSite> {
    let function = node.child_by_field_name("function").or_else(|| node.named_child(0))?;
    // A call in a chain sits on the line of its name, where a compiler reports it, not where the chain starts.
    let mut at = function;
    let (name, qualifier) = match function.kind() {
        "identifier" => (text(function, src).trim().to_string(), None),
        "generic_name" => (first_identifier(function, src)?, None),
        "qualified_name" => {
            let (name, qualifier) = split_qualified(&collapse(text(function, src)));
            (name, qualifier)
        }
        "member_access_expression" => {
            let name_node = function.child_by_field_name("name")?;
            let receiver = function.child_by_field_name("expression")?;
            at = name_node;
            let name = if name_node.kind() == "generic_name" {
                first_identifier(name_node, src)?
            } else {
                text(name_node, src).trim().to_string()
            };
            (name, Some(collapse(text(receiver, src))))
        }
        _ => return None,
    };
    if name.is_empty() {
        return None;
    }
    let line = line_of(at.start_position().row);
    Some(RefSite {
        name,
        qualifier,
        kind: RefKind::Calls,
        line,
        scope: scope_at(decls, line),
    })
}

fn creation_site(node: Node, src: &str, decls: &[Decl]) -> Option<RefSite> {
    let type_node = node.child_by_field_name("type").or_else(|| node.named_child(0))?;
    let (name, qualifier) = split_qualified(&collapse(text(type_node, src)));
    if name.is_empty() {
        return None;
    }
    let line = line_of(type_node.start_position().row);
    Some(RefSite {
        name,
        qualifier,
        kind: RefKind::Calls,
        line,
        scope: scope_at(decls, line),
    })
}

fn collect_bases(node: Node, src: &str, decls: &[Decl], refs: &mut Vec<RefSite>) {
    let enclosing = node.parent().map(|parent| parent.kind()).unwrap_or("");
    let first_inherits = matches!(
        enclosing,
        "class_declaration" | "record_declaration" | "record_struct_declaration"
    );
    let is_interface = enclosing == "interface_declaration";
    let mut index = 0;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let type_node = if child.kind() == "primary_constructor_base_type" {
            match child.named_child(0) {
                Some(inner) => inner,
                None => continue,
            }
        } else {
            child
        };
        let (name, qualifier) = split_qualified(&collapse(text(type_node, src)));
        if name.is_empty() {
            continue;
        }
        let kind = if is_interface || (first_inherits && index == 0) {
            RefKind::Inherits
        } else {
            RefKind::Implements
        };
        let line = line_of(child.start_position().row);
        refs.push(RefSite {
            name,
            qualifier,
            kind,
            line,
            scope: scope_at(decls, line),
        });
        index += 1;
    }
}

fn parse_using(node: Node, src: &str) -> Option<ImportDecl> {
    let text = collapse(text(node, src));
    let text = text.strip_suffix(';').unwrap_or(&text).trim();
    let global = text.starts_with("global ");
    let text = text.strip_prefix("global ").unwrap_or(text).trim();
    let text = text.strip_prefix("using ").unwrap_or(text).trim();
    let (is_static, text) = match text.strip_prefix("static ") {
        Some(rest) => (true, rest.trim()),
        None => (false, text),
    };
    if text.is_empty() {
        return None;
    }
    let (alias, path) = match text.split_once('=') {
        Some((alias, path)) => (Some(squeeze(alias)), squeeze(path)),
        None => (None, squeeze(text)),
    };
    let glob = path.ends_with(".*");
    let path = path.strip_suffix(".*").unwrap_or(&path).to_string();
    if path.is_empty() {
        return None;
    }
    Some(ImportDecl {
        path,
        alias,
        glob,
        is_static,
        global,
        line: line_of(node.start_position().row),
    })
}

// ---------------------------------------------------------------- helpers

pub(crate) fn scope_at(decls: &[Decl], line: u32) -> Option<usize> {
    decls
        .iter()
        .rposition(|decl| decl.line_start <= line && line <= decl.line_end)
}

pub(crate) fn split_qualified(text: &str) -> (String, Option<String>) {
    let mut depth = 0i32;
    let mut last_dot = None;
    for (index, c) in text.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            '.' if depth == 0 => last_dot = Some(index),
            _ => {}
        }
    }
    let (qualifier, name) = match last_dot {
        Some(index) => (text[..index].trim(), text[index + 1..].trim()),
        None => ("", text.trim()),
    };
    let name = match name.find('<') {
        Some(index) => name[..index].trim(),
        None => name,
    };
    let qualifier = qualifier.trim();
    (
        name.to_string(),
        if qualifier.is_empty() {
            None
        } else {
            Some(qualifier.to_string())
        },
    )
}

fn first_identifier(node: Node, src: &str) -> Option<String> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier");
    found.map(|child| text(child, src).trim().to_string())
}

pub(crate) fn text<'a>(node: Node, src: &'a str) -> &'a str {
    text_range(src, node.byte_range())
}

fn text_range(src: &str, range: std::ops::Range<usize>) -> &str {
    src.get(range).unwrap_or("")
}

pub(crate) fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect()
}

pub(crate) fn line_of(row: usize) -> u32 {
    row as u32 + 1
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
