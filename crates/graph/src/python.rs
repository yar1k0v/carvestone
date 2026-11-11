//! Python extraction: classes, functions, methods, module and class level
//! assignments, imports and the reference sites a resolver needs.
//!
//! Conventions the resolver reads back:
//! - qnames are the dotted module path plus the names: `pkg.mod.Class.method`
//!   (the path after the last `src` directory; `__init__` names its package);
//! - `import a.b.c` is an [`ImportDecl`] with `path = "a.b.c"` (and an alias for
//!   `as x`); a `from m import n as k` is `path = "m#n"`, `alias = Some("k")`,
//!   `m` as written (`.`, `..pkg`); `from m import *` has `glob = true`;
//! - an `Imports` site has the module as qualifier and the imported name
//!   (`*` for a plain import or a wildcard);
//! - a member call has qualifier `.<receiver text>`, a plain call has none; a
//!   base class or type reference carries a dotted path.

use tree_sitter::{Node, Parser};

use crate::csharp::{collapse, line_of, scope_at, text};
use crate::facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning};

/// Deeper trees are truncated: a declaration is never worth a stack overflow.
const MAX_DEPTH: usize = 128;
/// A receiver longer than this is not a name path worth keeping.
const MAX_RECEIVER: usize = 120;

struct Sink {
    decls: Vec<Decl>,
    refs: Vec<RefSite>,
    imports: Vec<ImportDecl>,
    warnings: Vec<Warning>,
    module: String,
}

pub(crate) fn extract_python(path: &str, src: &str) -> FileFacts {
    let mut facts = FileFacts::new(Lang::Python, path);
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_python::LANGUAGE.into()).is_err() {
        facts.warnings.push(Warning {
            message: "Python grammar unavailable".to_string(),
            line: 1,
        });
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        facts.warnings.push(Warning {
            message: "Python parser returned no tree".to_string(),
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
            doc: docstring(root, src),
            line_start: 1,
            line_end: src.lines().count().max(1) as u32,
        }],
        refs: Vec::new(),
        imports: Vec::new(),
        warnings: Vec::new(),
        module: module_name(path),
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
        decl_of(child, child, src, parent, depth, out);
    }
}

fn decl_of(node: Node, outer: Node, src: &str, parent: usize, depth: usize, out: &mut Sink) {
    match node.kind() {
        "decorated_definition" => {
            if let Some(definition) = node.child_by_field_name("definition") {
                decl_of(definition, node, src, parent, depth + 1, out);
            }
        }
        "function_definition" => {
            let name = name_of(node, src);
            if name.is_empty() {
                return;
            }
            let kind = if out.decls[parent].kind == DeclKind::Type {
                DeclKind::Method
            } else {
                DeclKind::Function
            };
            // Functions inside functions are not declarations of the graph.
            if matches!(out.decls[parent].kind, DeclKind::Function | DeclKind::Method) {
                return;
            }
            push(out, node, outer, src, &name, kind, parent, header_text(node, src));
        }
        "class_definition" => {
            let name = name_of(node, src);
            if name.is_empty() {
                return;
            }
            if matches!(out.decls[parent].kind, DeclKind::Function | DeclKind::Method) {
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
                header_text(node, src),
            );
            if let Some(body) = node.child_by_field_name("body") {
                walk_decls(body, src, index, depth + 1, out);
            }
        }
        "expression_statement" => {
            if matches!(out.decls[parent].kind, DeclKind::Function | DeclKind::Method) {
                return;
            }
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            if let Some(assignment) = first
                && assignment.kind() == "assignment"
                && let Some(left) = assignment.child_by_field_name("left")
                && left.kind() == "identifier"
            {
                let name = text(left, src).trim().to_string();
                let mut header = collapse(text(assignment, src));
                if header.len() > 120 {
                    header = header.chars().take(120).collect();
                }
                push(out, node, outer, src, &name, DeclKind::Member, parent, header);
            }
        }
        "if_statement"
        | "try_statement"
        | "with_statement"
        | "for_statement"
        | "while_statement"
        | "elif_clause"
        | "else_clause"
        | "except_clause"
        | "except_group_clause"
        | "finally_clause"
        | "block"
        | "match_statement"
        | "case_clause" => {
            walk_decls(node, src, parent, depth + 1, out);
        }
        _ => {}
    }
}

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
        if out.module.is_empty() {
            name.to_string()
        } else {
            format!("{}.{name}", out.module)
        }
    } else {
        format!("{}.{name}", out.decls[parent].qname)
    };
    let doc = if matches!(kind, DeclKind::Function | DeclKind::Method | DeclKind::Type) {
        node.child_by_field_name("body").and_then(|b| docstring(b, src))
    } else {
        None
    };
    if let Some(existing) = out.decls.iter().rposition(|d| d.qname == qname && d.kind == kind) {
        // A redefinition (property setters, conditional defs) widens the first.
        let decl = &mut out.decls[existing];
        decl.line_end = decl.line_end.max(line_of(outer.end_position().row));
        return existing;
    }
    out.decls.push(Decl {
        name: name.to_string(),
        qname,
        kind,
        parent: Some(parent),
        signature,
        doc,
        line_start: line_of(outer.start_position().row),
        line_end: line_of(outer.end_position().row),
    });
    out.decls.len() - 1
}

fn name_of(node: Node, src: &str) -> String {
    node.child_by_field_name("name")
        .map(|n| text(n, src).trim().to_string())
        .unwrap_or_default()
}

/// The definition header: everything before the body, whitespace collapsed.
fn header_text(node: Node, src: &str) -> String {
    let start = node.start_byte();
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    if end <= start {
        return String::new();
    }
    let header = collapse(src.get(start..end).unwrap_or(""));
    header.trim().trim_end_matches(':').trim().to_string()
}

/// The first statement of a body when it is a string literal.
fn docstring(body: Node, src: &str) -> Option<String> {
    let mut cursor = body.walk();
    let first = body.named_children(&mut cursor).next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let mut inner = first.walk();
    let string = first.named_children(&mut inner).next()?;
    if string.kind() != "string" {
        return None;
    }
    let raw = text(string, src).trim();
    let body = ["\"\"\"", "'''", "\"", "'"]
        .iter()
        .find_map(|quote| raw.strip_prefix(quote).and_then(|r| r.strip_suffix(quote)))
        .unwrap_or(raw);
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    let joined = lines.join("\n").trim().to_string();
    if joined.is_empty() { None } else { Some(joined) }
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
        "import_from_statement" => {
            import_from(node, src, out);
            return;
        }
        "call" => {
            if let Some(function) = node.child_by_field_name("function") {
                match function.kind() {
                    "identifier" => {
                        let name = text(function, src).trim().to_string();
                        push_site(out, name, None, RefKind::Calls, node);
                    }
                    "attribute" => {
                        if let (Some(object), Some(attribute)) = (
                            function.child_by_field_name("object"),
                            function.child_by_field_name("attribute"),
                        ) {
                            let receiver = collapse(text(object, src));
                            let receiver = if receiver.len() > MAX_RECEIVER {
                                String::new()
                            } else {
                                receiver
                            };
                            let name = text(attribute, src).trim().to_string();
                            // On the line of the name, where a chained call is reported.
                            push_site(out, name, Some(format!(".{receiver}")), RefKind::Calls, attribute);
                        }
                    }
                    _ => {}
                }
            }
        }
        "class_definition" => {
            if let Some(bases) = node.child_by_field_name("superclasses") {
                let mut cursor = bases.walk();
                let items: Vec<Node> = bases.named_children(&mut cursor).collect();
                for item in items {
                    let target = if item.kind() == "subscript" {
                        item.child_by_field_name("value").unwrap_or(item)
                    } else {
                        item
                    };
                    if let Some((name, qualifier)) = dotted_parts(target, src) {
                        push_site(out, name, qualifier, RefKind::Inherits, target);
                    }
                }
            }
        }
        "decorator" => {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            if let Some(expression) = first
                && matches!(expression.kind(), "identifier" | "attribute")
                && let Some((name, qualifier)) = dotted_parts(expression, src)
            {
                push_site(out, name, qualifier.map(|q| format!(".{q}")), RefKind::Calls, node);
            }
        }
        "type" => {
            type_refs(node, src, out, depth + 1);
            return;
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
    if name.is_empty() {
        return;
    }
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

/// `(name, dotted qualifier)` of `Foo` or `pkg.mod.Foo`.
fn dotted_parts(node: Node, src: &str) -> Option<(String, Option<String>)> {
    match node.kind() {
        "identifier" => {
            let name = text(node, src).trim().to_string();
            if name.is_empty() { None } else { Some((name, None)) }
        }
        "attribute" => {
            let name = text(node.child_by_field_name("attribute")?, src).trim().to_string();
            let qualifier = collapse(text(node.child_by_field_name("object")?, src));
            if name.is_empty() || qualifier.is_empty() || qualifier.len() > MAX_RECEIVER {
                return None;
            }
            Some((name, Some(qualifier)))
        }
        _ => None,
    }
}

fn type_refs(node: Node, src: &str, out: &mut Sink, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match node.kind() {
        "identifier" | "attribute" => {
            if let Some((name, qualifier)) = dotted_parts(node, src) {
                push_site(out, name, qualifier, RefKind::References, node);
            }
        }
        "string" => {}
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_refs(child, src, out, depth + 1);
            }
        }
    }
}

// ---------------------------------------------------------------- imports

fn import_statement(node: Node, src: &str, out: &mut Sink) {
    let line = line_of(node.start_position().row);
    let mut cursor = node.walk();
    let names: Vec<Node> = node.children_by_field_name("name", &mut cursor).collect();
    for item in names {
        let (module, alias) = match item.kind() {
            "dotted_name" => (collapse(text(item, src)), None),
            "aliased_import" => {
                let Some(name) = item.child_by_field_name("name") else {
                    continue;
                };
                let alias = item
                    .child_by_field_name("alias")
                    .map(|a| text(a, src).trim().to_string());
                (collapse(text(name, src)), alias)
            }
            _ => continue,
        };
        let module = module.replace(' ', "");
        if module.is_empty() {
            continue;
        }
        out.imports.push(ImportDecl {
            path: module.clone(),
            alias,
            glob: false,
            is_static: false,
            global: false,
            line,
        });
        out.refs.push(RefSite {
            name: "*".to_string(),
            qualifier: Some(module),
            kind: RefKind::Imports,
            line,
            scope: Some(0),
        });
    }
}

fn import_from(node: Node, src: &str, out: &mut Sink) {
    let line = line_of(node.start_position().row);
    let Some(module_node) = node.child_by_field_name("module_name") else {
        return;
    };
    let module = collapse(text(module_node, src)).replace(' ', "");
    if module.is_empty() {
        return;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children_by_field_name("name", &mut cursor).collect();
    let mut any = false;
    for item in children {
        let (name, alias) = match item.kind() {
            "dotted_name" => (collapse(text(item, src)), None),
            "aliased_import" => {
                let Some(name) = item.child_by_field_name("name") else {
                    continue;
                };
                (
                    collapse(text(name, src)),
                    item.child_by_field_name("alias")
                        .map(|a| text(a, src).trim().to_string()),
                )
            }
            _ => continue,
        };
        let name = name.replace(' ', "");
        if name.is_empty() {
            continue;
        }
        any = true;
        let local = alias.unwrap_or_else(|| name.clone());
        out.imports.push(ImportDecl {
            path: format!("{module}#{name}"),
            alias: Some(local),
            glob: false,
            is_static: false,
            global: false,
            line,
        });
        out.refs.push(RefSite {
            name,
            qualifier: Some(module.clone()),
            kind: RefKind::Imports,
            line,
            scope: Some(0),
        });
    }
    let mut c = node.walk();
    let wildcard = node.children(&mut c).any(|child| child.kind() == "wildcard_import");
    if wildcard {
        any = true;
        out.imports.push(ImportDecl {
            path: module.clone(),
            alias: None,
            glob: true,
            is_static: false,
            global: false,
            line,
        });
        out.refs.push(RefSite {
            name: "*".to_string(),
            qualifier: Some(module.clone()),
            kind: RefKind::Imports,
            line,
            scope: Some(0),
        });
    }
    if !any {
        // `from m import (` cut short: the module is still a dependency.
        out.refs.push(RefSite {
            name: "*".to_string(),
            qualifier: Some(module),
            kind: RefKind::Imports,
            line,
            scope: Some(0),
        });
    }
}

// ---------------------------------------------------------------- paths

/// The dotted module of a file: the path after the last `src` directory,
/// `__init__` naming its package.
fn module_name(path: &str) -> String {
    let normal = path.replace('\\', "/");
    let parts: Vec<&str> = normal.split('/').filter(|p| !p.is_empty()).collect();
    let start = parts
        .iter()
        .rposition(|part| *part == "src")
        .map_or(0, |index| (index + 1).min(parts.len().saturating_sub(1)));
    let mut segments: Vec<String> = parts[start..].iter().map(|p| (*p).to_string()).collect();
    if let Some(last) = segments.last_mut() {
        *last = last.strip_suffix(".py").unwrap_or(last).to_string();
        if last == "__init__" {
            segments.pop();
        }
    }
    segments.join(".")
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
