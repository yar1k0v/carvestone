//! Rust extraction: modules, types, functions, impl methods, `use` trees and
//! the reference sites a resolver needs. tree-sitter recovers from broken
//! input, so a parse error costs the broken item, not the file.
//!
//! Conventions the resolver reads back:
//! - a `mod x;` declaration is a `Module` decl whose signature ends in `;`
//!   (with a leading `#[path = "..."]` when the attribute is present); an
//!   inline module has no `;`;
//! - impl methods are `Method` decls named `<prefix>::Type::name`;
//! - a method call site has a qualifier starting with `.` (the receiver text);
//!   a macro call has a qualifier starting with `!`; a path call has the path;
//! - every `use` leaf gives an [`ImportDecl`] and an `Imports` site.

use tree_sitter::{Node, Parser};

use crate::csharp::{collapse, line_of, scope_at, text};
use crate::facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning};

/// Deeper trees are truncated: a declaration is never worth a stack overflow.
const MAX_DEPTH: usize = 128;

const TYPE_KINDS: &[&str] = &["struct_item", "union_item", "enum_item", "trait_item", "type_item"];

pub(crate) fn extract_rust(path: &str, src: &str) -> FileFacts {
    let mut facts = FileFacts::new(Lang::Rust, path);
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_err() {
        facts.warnings.push(Warning {
            message: "Rust grammar unavailable".to_string(),
            line: 1,
        });
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        facts.warnings.push(Warning {
            message: "Rust parser returned no tree".to_string(),
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
    };
    walk_items(root, src, &module_prefix(path), Some(0), 0, &mut sink);

    let mut imports = Vec::new();
    let mut warnings = Vec::new();
    walk_sites(root, src, &mut sink, &mut imports, &mut warnings, 0);
    facts.decls = sink.decls;
    facts.refs = sink.refs;
    facts.imports = imports;
    warnings.sort_by_key(|warning| warning.line);
    warnings.dedup_by_key(|warning| warning.line);
    facts.warnings.extend(warnings);
    facts
}

struct Sink {
    decls: Vec<Decl>,
    refs: Vec<RefSite>,
}

// ---------------------------------------------------------------- declarations

/// The module path a file sits at by its location alone: what follows the last
/// `src`, or starts at the last `tests`, `examples` or `benches` directory.
/// `lib.rs`, `main.rs` and `mod.rs` name their directory.
fn module_prefix(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|part| !part.is_empty()).collect();
    let start = parts
        .iter()
        .rposition(|part| matches!(*part, "src" | "tests" | "examples" | "benches"))
        .map_or(parts.len().saturating_sub(1), |index| {
            if parts[index] == "src" { index + 1 } else { index }
        });
    let mut segments: Vec<String> = parts[start..].iter().map(|part| (*part).to_string()).collect();
    if let Some(last) = segments.last_mut() {
        *last = last.strip_suffix(".rs").unwrap_or(last).to_string();
        if matches!(last.as_str(), "lib" | "main" | "mod") {
            segments.pop();
        }
    }
    segments
}

fn join(prefix: &[String], name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", prefix.join("::"))
    }
}

fn walk_items(node: Node, src: &str, prefix: &[String], parent: Option<usize>, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in &children {
        let kind = child.kind();
        if kind == "mod_item" {
            let name = name_of(*child, src);
            if name.is_empty() {
                continue;
            }
            let body = child.child_by_field_name("body");
            let mut signature = header_text(*child, src, true);
            if body.is_none() && !signature.ends_with(';') {
                signature.push(';');
            }
            if let Some(path) = path_attr(*child, src) {
                signature = format!("#[path = \"{path}\"] {signature}");
            }
            let index = push(
                out,
                *child,
                src,
                &name,
                join(prefix, &name),
                DeclKind::Module,
                parent,
                signature,
            );
            if let Some(body) = body {
                let mut inner = prefix.to_vec();
                inner.push(name);
                walk_items(body, src, &inner, Some(index), depth + 1, out);
            }
        } else if TYPE_KINDS.contains(&kind) {
            let name = name_of(*child, src);
            if name.is_empty() {
                continue;
            }
            let signature = header_text(*child, src, false);
            let index = push(
                out,
                *child,
                src,
                &name,
                join(prefix, &name),
                DeclKind::Type,
                parent,
                signature,
            );
            if kind == "enum_item" {
                push_variants(*child, src, index, out);
            } else if kind == "trait_item" {
                push_supertraits(*child, src, index, out);
                if let Some(body) = child.child_by_field_name("body") {
                    let owner_qname = out.decls[index].qname.clone();
                    push_assoc(body, src, &owner_qname, index, out);
                }
            }
        } else if kind == "function_item" {
            let name = name_of(*child, src);
            if name.is_empty() {
                continue;
            }
            let signature = header_text(*child, src, false);
            push(
                out,
                *child,
                src,
                &name,
                join(prefix, &name),
                DeclKind::Function,
                parent,
                signature,
            );
        } else if matches!(kind, "const_item" | "static_item") {
            let name = name_of(*child, src);
            if name.is_empty() {
                continue;
            }
            let signature = header_text(*child, src, false);
            push(
                out,
                *child,
                src,
                &name,
                join(prefix, &name),
                DeclKind::Member,
                parent,
                signature,
            );
        } else if kind == "macro_definition" {
            let name = name_of(*child, src);
            if name.is_empty() {
                continue;
            }
            let signature = format!("macro_rules! {name}");
            push(
                out,
                *child,
                src,
                &name,
                join(prefix, &name),
                DeclKind::Function,
                parent,
                signature,
            );
        }
    }
    // Impls last: the type they extend is in `out` by then, wherever it sits.
    for child in &children {
        if child.kind() == "impl_item" {
            walk_impl(*child, src, prefix, parent, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Sink,
    node: Node,
    src: &str,
    name: &str,
    qname: String,
    kind: DeclKind,
    parent: Option<usize>,
    signature: String,
) -> usize {
    out.decls.push(Decl {
        name: name.to_string(),
        qname,
        kind,
        parent,
        signature,
        doc: doc_above(node, src),
        line_start: line_of(node.start_position().row),
        line_end: line_of(node.end_position().row),
    });
    out.decls.len() - 1
}

fn push_variants(node: Node, src: &str, owner: usize, out: &mut Sink) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    for variant in body.named_children(&mut cursor) {
        if variant.kind() != "enum_variant" {
            continue;
        }
        let name = name_of(variant, src);
        if name.is_empty() {
            continue;
        }
        let qname = format!("{}::{name}", out.decls[owner].qname);
        let signature = collapse(text(variant, src));
        push(
            out,
            variant,
            src,
            &name,
            qname,
            DeclKind::Member,
            Some(owner),
            signature,
        );
    }
}

fn push_supertraits(node: Node, src: &str, owner: usize, out: &mut Sink) {
    let Some(bounds) = node.child_by_field_name("bounds") else {
        return;
    };
    let mut cursor = bounds.walk();
    for bound in bounds.named_children(&mut cursor) {
        if let Some((name, qualifier)) = path_parts(bound, src) {
            out.refs.push(RefSite {
                name,
                qualifier,
                kind: RefKind::Inherits,
                line: line_of(bound.start_position().row),
                scope: Some(owner),
            });
        }
    }
}

/// Functions and constants inside a trait or impl body, named `Owner::item`.
fn push_assoc(body: Node, src: &str, owner_qname: &str, parent: usize, out: &mut Sink) {
    let mut cursor = body.walk();
    for item in body.named_children(&mut cursor) {
        let kind = match item.kind() {
            "function_item" | "function_signature_item" => DeclKind::Method,
            "const_item" => DeclKind::Member,
            _ => continue,
        };
        let name = name_of(item, src);
        if name.is_empty() {
            continue;
        }
        let signature = header_text(item, src, false);
        push(
            out,
            item,
            src,
            &name,
            format!("{owner_qname}::{name}"),
            kind,
            Some(parent),
            signature,
        );
    }
}

fn walk_impl(node: Node, src: &str, prefix: &[String], parent: Option<usize>, out: &mut Sink) {
    let Some(type_node) = node.child_by_field_name("type") else {
        return;
    };
    let type_name = path_parts(type_node, src)
        .map(|(name, _)| name)
        .unwrap_or_else(|| collapse(text(type_node, src)));
    let local = out
        .decls
        .iter()
        .position(|decl| decl.kind == DeclKind::Type && decl.name == type_name && decl.parent == parent);
    // Methods are named after the impl'd type even when it lives elsewhere.
    let (owner_qname, method_parent) = match local {
        Some(index) => (out.decls[index].qname.clone(), index),
        None => (join(prefix, &type_name), parent.unwrap_or(0)),
    };
    if let Some(trait_node) = node.child_by_field_name("trait")
        && let Some((name, qualifier)) = path_parts(trait_node, src)
    {
        out.refs.push(RefSite {
            name,
            qualifier,
            kind: RefKind::Implements,
            line: line_of(node.start_position().row),
            scope: local,
        });
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    push_assoc(body, src, &owner_qname, method_parent, out);
}

fn name_of(node: Node, src: &str) -> String {
    node.child_by_field_name("name")
        .map(|name| text(name, src).trim().to_string())
        .unwrap_or_default()
}

/// The item header: everything before the body, whitespace collapsed.
fn header_text(node: Node, src: &str, keep_semicolon: bool) -> String {
    let start = node.start_byte();
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    if end <= start {
        return String::new();
    }
    let header = collapse(src.get(start..end).unwrap_or(""));
    if keep_semicolon {
        header.trim().to_string()
    } else {
        header.trim_end_matches(';').trim().to_string()
    }
}

/// The string of a `#[path = "..."]` attribute right above `node`.
fn path_attr(node: Node, src: &str) -> Option<String> {
    let mut current = node;
    while let Some(prev) = current.prev_sibling() {
        match prev.kind() {
            "attribute_item" => {
                let attr = collapse(text(prev, src));
                if let Some(rest) = attr.strip_prefix("#[path")
                    && let Some(open) = rest.find('"')
                    && let Some(len) = rest[open + 1..].find('"')
                {
                    return Some(rest[open + 1..open + 1 + len].to_string());
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        current = prev;
    }
    None
}

/// The `///` or `/** */` block right above `node`, attributes in between allowed.
fn doc_above(node: Node, src: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut expected = node.start_position().row;
    let mut current = node;
    while let Some(prev) = current.prev_sibling() {
        if prev.end_position().row + 1 < expected {
            break;
        }
        match prev.kind() {
            "attribute_item" => {}
            "line_comment" => {
                let raw = text(prev, src).trim();
                match raw.strip_prefix("///") {
                    Some(body) if !body.starts_with('/') => lines.push(body.trim().to_string()),
                    _ => break,
                }
            }
            "block_comment" => {
                let raw = text(prev, src).trim();
                let Some(body) = raw.strip_prefix("/**") else { break };
                let body = body.strip_suffix("*/").unwrap_or(body);
                let block: Vec<String> = body
                    .lines()
                    .map(|line| line.trim().trim_start_matches('*').trim().to_string())
                    .collect();
                lines.extend(block.into_iter().rev());
            }
            _ => break,
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

/// `(name, qualifier)` of a path-like node: `Foo`, `a::Foo`, `Foo<T>`, `f::<T>`.
fn path_parts(node: Node, src: &str) -> Option<(String, Option<String>)> {
    match node.kind() {
        "identifier" | "type_identifier" => {
            let name = text(node, src).trim().to_string();
            if name.is_empty() { None } else { Some((name, None)) }
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let name = text(node.child_by_field_name("name")?, src).trim().to_string();
            let raw = collapse(text(node.child_by_field_name("path")?, src));
            if name.is_empty() || raw.starts_with('<') {
                return None;
            }
            let qualifier = strip_generics(&raw).trim_matches(':').trim().to_string();
            Some((name, if qualifier.is_empty() { None } else { Some(qualifier) }))
        }
        "generic_type" => path_parts(node.child_by_field_name("type")?, src),
        "generic_function" => path_parts(node.child_by_field_name("function")?, src),
        _ => None,
    }
}

/// `Vec::<T>::new` becomes `Vec::new`; also removes `<...>` in `Foo<T>`.
fn strip_generics(text: &str) -> String {
    let mut depth = 0usize;
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.replace("::::", "::")
}

fn walk_sites(
    node: Node,
    src: &str,
    out: &mut Sink,
    imports: &mut Vec<ImportDecl>,
    warnings: &mut Vec<Warning>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let kind = node.kind();
    match kind {
        "use_declaration" => {
            collect_use(node, src, out, imports);
            return;
        }
        "attribute_item" | "inner_attribute_item" => return,
        _ => {}
    }
    if node.is_error() || node.is_missing() {
        warnings.push(Warning {
            message: "syntax error".to_string(),
            line: line_of(node.start_position().row),
        });
    }
    // Children whose sites are emitted here (or are not references at all).
    let mut skip: Vec<usize> = Vec::new();
    match kind {
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                call_site(node, function, src, out);
                skip.push(function.id());
                walk_callee(function, src, out, imports, warnings, depth);
            }
        }
        "macro_invocation" => {
            if let Some(site) = macro_site(node, src, &out.decls) {
                out.refs.push(site);
            }
            if let Some(path) = node.child_by_field_name("macro") {
                skip.push(path.id());
            }
        }
        "struct_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                if let Some((name_text, qualifier)) = path_parts(name, src) {
                    push_reference(out, name_text, qualifier, name);
                }
                skip.push(name.id());
            }
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            if let Some((name, qualifier)) = path_parts(node, src) {
                push_reference(out, name, qualifier, node);
            }
            return;
        }
        "type_identifier" => {
            let name = text(node, src).trim();
            if name != "Self" && !name.is_empty() {
                push_reference(out, name.to_string(), None, node);
            }
            return;
        }
        // A const or static read is a bare identifier; the naming convention
        // keeps locals and functions out.
        "identifier" => {
            let name = text(node, src).trim();
            if is_const_name(name) {
                push_reference(out, name.to_string(), None, node);
            }
            return;
        }
        "const_item" | "static_item" | "const_parameter" => {
            if let Some(child) = node.child_by_field_name("name") {
                skip.push(child.id());
            }
        }
        "impl_item" => {
            for field in ["type", "trait"] {
                if let Some(child) = node.child_by_field_name(field) {
                    skip.push(child.id());
                }
            }
        }
        "trait_item" => {
            for field in ["name", "bounds"] {
                if let Some(child) = node.child_by_field_name(field) {
                    skip.push(child.id());
                }
            }
        }
        "struct_item" | "enum_item" | "union_item" | "type_item" => {
            if let Some(child) = node.child_by_field_name("name") {
                skip.push(child.id());
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in children {
        if !skip.contains(&child.id()) {
            walk_sites(child, src, out, imports, warnings, depth + 1);
        }
    }
}

/// `SCREAMING_SNAKE` names: at least 2 characters, an uppercase first letter.
fn is_const_name(name: &str) -> bool {
    name.len() >= 2
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// What a call's callee contributes besides the call itself: a method call's
/// receiver and a generic call's type arguments can hold more sites.
fn walk_callee(
    function: Node,
    src: &str,
    out: &mut Sink,
    imports: &mut Vec<ImportDecl>,
    warnings: &mut Vec<Warning>,
    depth: usize,
) {
    match function.kind() {
        "identifier" | "scoped_identifier" => {}
        "field_expression" => {
            if let Some(value) = function.child_by_field_name("value") {
                walk_sites(value, src, out, imports, warnings, depth + 1);
            }
        }
        "generic_function" => {
            if let Some(args) = function.child_by_field_name("type_arguments") {
                walk_sites(args, src, out, imports, warnings, depth + 1);
            }
            if let Some(inner) = function.child_by_field_name("function") {
                walk_callee(inner, src, out, imports, warnings, depth + 1);
            }
        }
        _ => walk_sites(function, src, out, imports, warnings, depth + 1),
    }
}

fn push_reference(out: &mut Sink, name: String, qualifier: Option<String>, node: Node) {
    let line = line_of(node.start_position().row);
    out.refs.push(RefSite {
        name,
        qualifier,
        kind: RefKind::References,
        line,
        scope: scope_at(&out.decls, line),
    });
}

fn call_site(call: Node, function: Node, src: &str, out: &mut Sink) {
    let mut line = line_of(call.start_position().row);
    let (name, qualifier) = match function.kind() {
        "field_expression" => {
            let Some(field) = function.child_by_field_name("field") else {
                return;
            };
            // On the line of the method name, where a chained call is reported.
            line = line_of(field.start_position().row);
            let receiver = function
                .child_by_field_name("value")
                .map(|value| collapse(text(value, src)))
                .unwrap_or_default();
            let receiver: String = receiver.chars().take(80).collect();
            (text(field, src).trim().to_string(), Some(format!(".{receiver}")))
        }
        _ => match path_parts(function, src) {
            Some(parts) => parts,
            None => return,
        },
    };
    if name.is_empty() {
        return;
    }
    out.refs.push(RefSite {
        name,
        qualifier,
        kind: RefKind::Calls,
        line,
        scope: scope_at(&out.decls, line),
    });
}

fn macro_site(node: Node, src: &str, decls: &[Decl]) -> Option<RefSite> {
    let path = node.child_by_field_name("macro")?;
    let (name, qualifier) = path_parts(path, src)?;
    let line = line_of(node.start_position().row);
    Some(RefSite {
        name,
        qualifier: Some(format!("!{}", qualifier.unwrap_or_default())),
        kind: RefKind::Calls,
        line,
        scope: scope_at(decls, line),
    })
}

struct UseLeaf {
    path: String,
    alias: Option<String>,
    glob: bool,
    line: u32,
}

fn collect_use(node: Node, src: &str, out: &mut Sink, imports: &mut Vec<ImportDecl>) {
    let mut leaves = Vec::new();
    if let Some(argument) = node.child_by_field_name("argument") {
        flatten_use(argument, src, "", &mut leaves, 0);
    }
    for leaf in leaves {
        let path = leaf.path.trim_start_matches("::").to_string();
        if path.is_empty() {
            continue;
        }
        let (qualifier, name) = match path.rsplit_once("::") {
            Some((qualifier, name)) => (Some(qualifier.to_string()), name.to_string()),
            None => (None, path.clone()),
        };
        out.refs.push(RefSite {
            name,
            qualifier,
            kind: RefKind::Imports,
            line: leaf.line,
            scope: None,
        });
        imports.push(ImportDecl {
            path,
            alias: leaf.alias,
            glob: leaf.glob,
            is_static: false,
            global: false,
            line: leaf.line,
        });
    }
}

fn join_path(prefix: &str, tail: &str) -> String {
    match (prefix.is_empty(), tail.is_empty()) {
        (true, _) => tail.to_string(),
        (_, true) => prefix.to_string(),
        _ => format!("{prefix}::{tail}"),
    }
}

fn flatten_use(node: Node, src: &str, prefix: &str, leaves: &mut Vec<UseLeaf>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let line = line_of(node.start_position().row);
    let squeeze = |text: &str| -> String { text.split_whitespace().collect() };
    match node.kind() {
        "identifier" | "scoped_identifier" | "crate" | "super" | "metavariable" => {
            leaves.push(UseLeaf {
                path: join_path(prefix, &squeeze(text(node, src))),
                alias: None,
                glob: false,
                line,
            });
        }
        "self" => leaves.push(UseLeaf {
            path: prefix.to_string(),
            alias: None,
            glob: false,
            line,
        }),
        "use_as_clause" => {
            let alias = node
                .child_by_field_name("alias")
                .map(|alias| text(alias, src).trim().to_string());
            let mut inner = Vec::new();
            if let Some(path) = node.child_by_field_name("path") {
                flatten_use(path, src, prefix, &mut inner, depth + 1);
            }
            for mut leaf in inner {
                leaf.alias = alias.clone().filter(|alias| alias != "_");
                leaves.push(leaf);
            }
        }
        "use_wildcard" => {
            let mut cursor = node.walk();
            let path = node
                .named_children(&mut cursor)
                .next()
                .map(|path| squeeze(text(path, src)))
                .unwrap_or_default();
            leaves.push(UseLeaf {
                path: join_path(prefix, &path),
                alias: None,
                glob: true,
                line,
            });
        }
        "scoped_use_list" => {
            let head = node
                .child_by_field_name("path")
                .map(|path| squeeze(text(path, src)))
                .unwrap_or_default();
            if let Some(list) = node.child_by_field_name("list") {
                flatten_use(list, src, &join_path(prefix, &head), leaves, depth + 1);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                flatten_use(child, src, prefix, leaves, depth + 1);
            }
        }
        _ => {}
    }
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
