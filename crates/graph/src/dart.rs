//! Dart extraction: classes, mixins, enums, extensions, extension types and typedefs as types;
//! functions, methods, getters, setters, operators, constructors, fields and enum entries; the
//! directives and the reference sites a resolver needs. The grammar is `tree-sitter-dart` 0.3.0
//! from crates.io.
//!
//! Conventions the resolver reads back:
//! - the file is declaration 0 (`qname` = repo path); every other `qname` is the repo path, `#`,
//!   then the names: `mobile/lib/auth.dart#AuthRepo.login`. Two projects never collide because
//!   the path is the first part;
//! - types (`class`, `mixin`, `enum`, `extension`, `extension type`, `typedef`) are
//!   [`DeclKind::Type`]; an unnamed extension is named `extension on <type>`;
//! - top-level functions, getters and setters are functions; the same inside a type are methods,
//!   and so are operators (`operator ==`) and constructors; a field, an enum entry and a
//!   top-level variable are members, one per name of `int a, b;`. A getter and its setter are two
//!   declarations under one `qname`;
//! - a constructor is named after what follows the dot: `Foo.named(...)` is `named`, the unnamed
//!   constructor (generative or factory) is `new`, as in `Foo.new`. The signature keeps the
//!   header (`Foo.named(this.a)`, `factory Foo.make()`), initializer lists cut off;
//! - a declaration's range starts at its first annotation and covers its body; `///` and `/** */`
//!   right above it are its doc;
//! - a method, getter, setter or operator declared without a body (`void run();`) has `abstract `
//!   in front of its signature, so dispatch can tell what an implementer must fill;
//! - generated files (`*.g.dart`, `*.freezed.dart`) never get here: `lang_of` skips them.
//!
//! Sites, in [`FileFacts::refs`] with the scope the declaration they sit in (the file for a
//! directive):
//! - `import`, `export`, `part` and `part of` are `Imports` sites with the URI as `name` (the
//!   dotted library name for `part of lib.name;`) and a `qualifier` of space-separated words: the
//!   directive (`import`, `export`, `part`, `part-of`), then any of `deferred`, `as=<prefix>`,
//!   `show=A,B`, `hide=A,B`. Only the first URI of a conditional import counts. `import`
//!   directives are also [`ImportDecl`]s: `path` is the URI, `alias` the prefix, `glob` true
//!   unless the import has a `show` list;
//! - `extends` (and the `on` of a mixin) are `Inherits` sites; `implements`, `with` and a mixin
//!   application's mixins are `Implements` sites; `name` is the type's last identifier, `qualifier`
//!   its import prefix when it has one (`m.Widget`);
//! - a plain call `f()`, `Foo()` is a `Calls` site without qualifier; a member call `a.b.c()` has
//!   `name` `c` and qualifier `.a.b` (a leading dot says: a member of what follows); `a?.c()`,
//!   `..c()` (receiver: the cascade target), `new Foo.named()` (name `named`, qualifier `.Foo`)
//!   and `new Foo()` (name `Foo`) are the same;
//! - a type written anywhere (declared, argument, `as`, `is`, generic argument) is a `References`
//!   site on its last identifier, with its import prefix as qualifier; so is an annotation, a
//!   type-like (capitalised) receiver that is not called (`Colors.red`: `Colors`), and a bare name
//!   passed as an argument (`ref.watch(authProvider)`, `onTap: _open`).
//!
//! Syntax the grammar rejects keeps the file: declarations and sites outside the error nodes (and
//! those the recovery still shaped inside them) are extracted, and one `Warning` per line with a
//! `syntax error` goes to the file's warning list.

use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Parser};

use crate::csharp::{collapse, line_of, text};
use crate::facts::{Decl, DeclKind, FileFacts, ImportDecl, Lang, RefKind, RefSite, Warning};

/// Deeper trees are truncated: a declaration is never worth a stack overflow.
pub(crate) const MAX_DEPTH: usize = 128;
/// A receiver longer than this is not a name path worth keeping.
const MAX_RECEIVER: usize = 120;
/// A signature longer than this is cut.
const MAX_SIGNATURE: usize = 200;

/// Types that name no declaration anyone writes in a repo.
const PRIMITIVES: &[&str] = &["int", "double", "num", "bool", "dynamic", "void", "var"];

struct Sink {
    decls: Vec<Decl>,
    refs: Vec<RefSite>,
    imports: Vec<ImportDecl>,
    warnings: Vec<Warning>,
    /// The declaration each declaring syntax node stands for, by node id.
    by_node: HashMap<usize, usize>,
    /// Type nodes already reported as a base type, so they are no second reference.
    claimed: HashSet<usize>,
}

/// The source with the two Dart forms the grammar does not know blanked out, byte for byte, so
/// every position stays: the `augment` of `import augment 'a.dart';` (read as a plain import of
/// the augmentation file) and the `macro` of `macro class M {}` (read as a plain class). Macros
/// were withdrawn from Dart and augmentations are experimental; `None` when the file has neither.
fn blank_withdrawn_syntax(src: &str) -> Option<String> {
    let mut out: Option<String> = None;
    let mut start = 0;
    for line in src.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let words: Vec<&str> = line.split_whitespace().take(6).collect();
        let at = match words.as_slice() {
            ["import", "augment", ..] => line[indent..].find("augment").map(|i| indent + i),
            _ => words
                .iter()
                .position(|w| *w == "class")
                .filter(|class| words[..*class].contains(&"macro"))
                .and_then(|_| {
                    let rest = &line[indent..];
                    rest.find("macro ").or_else(|| rest.find("macro\t")).map(|i| indent + i)
                }),
        };
        if let Some(at) = at {
            let word = if line[at..].starts_with("augment") {
                "augment"
            } else {
                "macro"
            };
            let text = out.get_or_insert_with(|| src.to_string());
            text.replace_range(start + at..start + at + word.len(), &" ".repeat(word.len()));
        }
        start += line.len();
    }
    out
}

pub(crate) fn extract_dart(path: &str, src: &str) -> FileFacts {
    let mut facts = FileFacts::new(Lang::Dart, path);
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let blanked = blank_withdrawn_syntax(src);
    let src = blanked.as_deref().unwrap_or(src);
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_dart::LANGUAGE.into()).is_err() {
        facts.warnings.push(Warning {
            message: "Dart grammar unavailable".to_string(),
            line: 1,
        });
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        facts.warnings.push(Warning {
            message: "Dart parser returned no tree".to_string(),
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
        by_node: HashMap::new(),
        claimed: HashSet::new(),
    };
    walk_decls(root, src, 0, 0, &mut sink);
    walk_sites(root, src, &mut sink, 0, 0);

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

pub(crate) fn kids(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// What a call calls. `_dio.get<dynamic>(..)` and `f<T>(..)` call an instantiation of the
/// function; the instantiation is looked through.
pub(crate) fn callee(call: Node) -> Option<Node> {
    let mut function = call.child_by_field_name("function")?;
    while function.kind() == "instantiation_expression" {
        function = function.child_by_field_name("function")?;
    }
    Some(function)
}

/// The receiver and the member of a function that is `a.b` or `a?.b`.
pub(crate) fn member_parts(function: Node) -> Option<(Node, Node)> {
    if !matches!(function.kind(), "member_expression" | "null_aware_member_expression") {
        return None;
    }
    Some((
        function.child_by_field_name("object")?,
        function.child_by_field_name("property")?,
    ))
}

fn walk_decls(node: Node, src: &str, parent: usize, depth: usize, out: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in kids(node) {
        decl_of(child, src, parent, depth, out);
    }
}

fn decl_of(node: Node, src: &str, parent: usize, depth: usize, out: &mut Sink) {
    match node.kind() {
        "class_declaration" => {
            let name = field_text(node, "name", src)
                .or_else(|| {
                    child_of(node, "mixin_application_class")
                        .and_then(|app| child_of(app, "identifier"))
                        .map(|id| text(id, src).trim().to_string())
                })
                .unwrap_or_default();
            type_decl(node, src, parent, depth, out, name, node.child_by_field_name("body"));
        }
        "mixin_declaration" | "enum_declaration" | "extension_type_declaration" | "extension_declaration" => {
            let name = match node.kind() {
                "extension_type_declaration" => node
                    .child_by_field_name("name")
                    .and_then(|n| child_of(n, "identifier").or(Some(n)))
                    .map(|n| text(n, src).trim().to_string())
                    .unwrap_or_default(),
                "extension_declaration" => field_text(node, "name", src).unwrap_or_else(|| {
                    let on = node
                        .child_by_field_name("class")
                        .map(|t| collapse(text(t, src)))
                        .unwrap_or_default();
                    format!("extension on {on}")
                }),
                _ => field_text(node, "name", src).unwrap_or_default(),
            };
            type_decl(node, src, parent, depth, out, name, node.child_by_field_name("body"));
        }
        "type_alias" => {
            let name = child_of(node, "type_identifier")
                .map(|n| text(n, src).trim().to_string())
                .unwrap_or_default();
            type_decl(node, src, parent, depth, out, name, None);
        }
        "function_declaration"
        | "getter_declaration"
        | "setter_declaration"
        | "external_function_declaration"
        | "external_getter_declaration"
        | "external_setter_declaration" => {
            let Some(sig) = node.child_by_field_name("signature") else {
                return;
            };
            let Some(name) = signature_name(sig, src) else {
                return;
            };
            let signature = clip(clean_text(sig, src, sig.end_byte()));
            push(out, node, src, &name, DeclKind::Function, parent, signature);
        }
        "top_level_variable_declaration" | "external_variable_declaration" => {
            fields(node, node, src, parent, out);
        }
        "member_declaration" => member(node, src, parent, out),
        "enum_entry" => {
            if let Some(name) = field_text(node, "name", src) {
                let signature = clip(clean_text(node, src, node.end_byte()));
                push(out, node, src, &name, DeclKind::Member, parent, signature);
            }
        }
        // Whatever the recovery still shaped inside an error keeps its declarations.
        "ERROR" => walk_decls(node, src, parent, depth + 1, out),
        _ => {}
    }
}

fn type_decl(node: Node, src: &str, parent: usize, depth: usize, out: &mut Sink, name: String, body: Option<Node>) {
    if name.is_empty() {
        return;
    }
    let end = body.map_or(node.end_byte(), |b| b.start_byte());
    let signature = clip(clean_text(node, src, end).trim_end_matches(';').trim().to_string());
    let index = push(out, node, src, &name, DeclKind::Type, parent, signature);
    if let Some(body) = body {
        walk_decls(body, src, index, depth + 1, out);
    }
}

/// A `member_declaration`: a method (with a body) or a `declaration` (a field, or a method or
/// constructor without a body).
fn member(node: Node, src: &str, parent: usize, out: &mut Sink) {
    for child in kids(node) {
        match child.kind() {
            "method_declaration" => {
                let Some(sig) = child.child_by_field_name("signature") else {
                    continue;
                };
                let inner = kids(sig).into_iter().find(|n| n.kind().ends_with("_signature"));
                let Some(name) = inner.and_then(|n| signature_name(n, src)) else {
                    continue;
                };
                let end = cut_at(sig, &["initializers"]);
                let signature = clip(clean_text(sig, src, end));
                push(out, node, src, &name, DeclKind::Method, parent, signature);
            }
            "declaration" => {
                let parts = kids(child);
                let inner = parts.iter().find(|n| n.kind().ends_with("_signature")).copied();
                if let Some(inner) = inner {
                    let Some(name) = signature_name(inner, src) else {
                        continue;
                    };
                    let end = cut_at(child, &["initializers", "redirection"]);
                    let mut signature = clean_text(child, src, end);
                    // A method or accessor without a body is the contract an implementer fills:
                    // say so, the way C# signatures do (dispatch reads the word).
                    let bodiless = matches!(
                        inner.kind(),
                        "function_signature" | "getter_signature" | "setter_signature" | "operator_signature"
                    ) && !signature
                        .split_whitespace()
                        .any(|word| matches!(word, "external" | "abstract"));
                    if bodiless {
                        signature = format!("abstract {signature}");
                    }
                    let signature = clip(signature);
                    push(out, node, src, &name, DeclKind::Method, parent, signature);
                } else {
                    fields(child, node, src, parent, out);
                }
            }
            _ => {}
        }
    }
}

/// One member per name of a field or variable declaration: `int a = 1, b;` gives `a` and `b`.
/// `outer` is the node whose range (metadata included) the members take.
fn fields(decl: Node, outer: Node, src: &str, parent: usize, out: &mut Sink) {
    let Some(list) = kids(decl).into_iter().find(|n| {
        matches!(
            n.kind(),
            "initialized_identifier_list" | "static_final_declaration_list" | "identifier_list"
        )
    }) else {
        return;
    };
    let prefix = clean_text(decl, src, list.start_byte());
    for item in kids(list) {
        let name = match item.kind() {
            "identifier" => text(item, src).trim().to_string(),
            _ => field_text(item, "name", src).unwrap_or_default(),
        };
        if name.is_empty() {
            continue;
        }
        let signature = clip(format!("{prefix} {name}").trim().to_string());
        push(out, outer, src, &name, DeclKind::Member, parent, signature);
    }
}

/// The name a signature declares. Constructors are `new` (unnamed) or the part after the dot.
fn signature_name(sig: Node, src: &str) -> Option<String> {
    let name = match sig.kind() {
        "function_signature" | "getter_signature" | "setter_signature" => field_text(sig, "name", src)?,
        "operator_signature" => {
            let op = sig.child_by_field_name("operator")?;
            format!("operator {}", text(op, src).trim())
        }
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => {
            let mut cursor = sig.walk();
            let names: Vec<String> = sig
                .children_by_field_name("name", &mut cursor)
                .map(|n| text(n, src).trim().to_string())
                .collect();
            match names.as_slice() {
                [] | [_] => "new".to_string(),
                [.., last] => last.clone(),
            }
        }
        _ => return None,
    };
    if name.is_empty() { None } else { Some(name) }
}

fn push(out: &mut Sink, node: Node, src: &str, name: &str, kind: DeclKind, parent: usize, signature: String) -> usize {
    let qname = if parent == 0 {
        format!("{}#{name}", out.decls[0].qname)
    } else {
        format!("{}.{name}", out.decls[parent].qname)
    };
    out.decls.push(Decl {
        name: name.to_string(),
        qname,
        kind,
        parent: Some(parent),
        signature,
        doc: doc_above(node, src),
        line_start: line_of(node.start_position().row),
        line_end: line_of(node.end_position().row),
    });
    let index = out.decls.len() - 1;
    out.by_node.entry(node.id()).or_insert(index);
    index
}

fn field_text(node: Node, field: &str, src: &str) -> Option<String> {
    let found = node.child_by_field_name(field)?;
    let name = text(found, src).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn child_of<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    kids(node).into_iter().find(|n| n.kind() == kind)
}

/// The byte where the first child of one of `kinds` starts, else the end of `node`.
fn cut_at(node: Node, kinds: &[&str]) -> usize {
    kids(node)
        .into_iter()
        .find(|n| kinds.contains(&n.kind()))
        .map_or(node.end_byte(), |n| n.start_byte())
}

/// The text of `node` from after its leading annotations and comments up to byte `end`,
/// whitespace collapsed.
fn clean_text(node: Node, src: &str, end: usize) -> String {
    let mut start = node.start_byte();
    for child in kids(node) {
        if matches!(child.kind(), "annotation" | "comment" | "documentation_comment") {
            start = start.max(child.end_byte());
        } else {
            break;
        }
    }
    let end = end.min(node.end_byte());
    if end <= start {
        return String::new();
    }
    collapse(src.get(start..end).unwrap_or("")).trim().to_string()
}

fn clip(signature: String) -> String {
    if signature.chars().count() <= MAX_SIGNATURE {
        return signature;
    }
    let mut cut: String = signature.chars().take(MAX_SIGNATURE).collect();
    cut.push_str("...");
    cut
}

/// The `///` lines or `/** */` block right above `node`, markers removed.
fn doc_above(node: Node, src: &str) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut expected = node.start_position().row;
    let mut current = node;
    while let Some(prev) = current.prev_sibling() {
        if prev.kind() != "documentation_comment" || prev.end_position().row + 1 < expected {
            break;
        }
        blocks.push(text(prev, src).trim().to_string());
        expected = prev.start_position().row;
        current = prev;
    }
    if blocks.is_empty() {
        return None;
    }
    blocks.reverse();
    let mut lines: Vec<String> = Vec::new();
    for raw in &blocks {
        if let Some(body) = raw.strip_prefix("///") {
            lines.push(body.strip_prefix(' ').unwrap_or(body).trim_end().to_string());
        } else if let Some(body) = raw.strip_prefix("/**") {
            let body = body.strip_suffix("*/").unwrap_or(body);
            lines.extend(body.lines().map(|line| {
                let line = line.trim();
                let line = line.strip_prefix('*').unwrap_or(line);
                line.strip_prefix(' ').unwrap_or(line).trim_end().to_string()
            }));
        }
    }
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

// ---------------------------------------------------------------- sites

fn walk_sites(node: Node, src: &str, out: &mut Sink, scope: usize, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if node.is_error() || node.is_missing() {
        out.warnings.push(Warning {
            message: if node.is_missing() {
                format!("syntax error: missing {}", node.kind())
            } else {
                "syntax error".to_string()
            },
            line: line_of(node.start_position().row),
        });
    }
    let scope = out.by_node.get(&node.id()).copied().unwrap_or(scope);
    match node.kind() {
        "library_import" | "library_export" | "part_directive" | "part_of_directive" => {
            directive(node, src, out);
            return;
        }
        "class_declaration" | "mixin_declaration" | "enum_declaration" | "extension_type_declaration" => {
            heritage(node, src, out, scope);
        }
        "call_expression" => call(node, src, out, scope),
        "cascade_call_expression" => cascade_call(node, src, out, scope),
        "new_expression" | "const_object_expression" | "constructor_invocation" => construction(node, src, out, scope),
        "type" => {
            if !out.claimed.contains(&node.id()) {
                type_site(node, src, out, scope, RefKind::References);
            }
        }
        "member_expression" | "null_aware_member_expression" => static_receiver(node, src, out, scope),
        "annotation" => annotation(node, src, out, scope),
        "arguments" | "named_argument" => bare_arguments(node, src, out, scope),
        _ => {}
    }
    for child in kids(node) {
        walk_sites(child, src, out, scope, depth + 1);
    }
}

fn push_site(out: &mut Sink, name: String, qualifier: Option<String>, kind: RefKind, at: Node, scope: usize) {
    if name.is_empty() {
        return;
    }
    out.refs.push(RefSite {
        name,
        qualifier,
        kind,
        line: line_of(at.start_position().row),
        scope: Some(scope),
    });
}

fn receiver_text(node: Node, src: &str) -> String {
    let receiver = collapse(text(node, src));
    if receiver.len() > MAX_RECEIVER {
        String::new()
    } else {
        receiver
    }
}

fn call(node: Node, src: &str, out: &mut Sink, scope: usize) {
    let Some(function) = callee(node) else {
        return;
    };
    if function.kind() == "identifier" {
        let name = text(function, src).trim().to_string();
        push_site(out, name, None, RefKind::Calls, function, scope);
    } else if let Some((object, property)) = member_parts(function) {
        let name = text(property, src).trim().to_string();
        let qualifier = Some(format!(".{}", receiver_text(object, src)));
        push_site(out, name, qualifier, RefKind::Calls, property, scope);
    }
}

fn cascade_call(node: Node, src: &str, out: &mut Sink, scope: usize) {
    let Some(property) = node.child_by_field_name("property") else {
        return;
    };
    // cascade_call_expression < cascade_section < cascade, whose first child is the target.
    let target = node
        .parent()
        .and_then(|section| section.parent())
        .filter(|cascade| cascade.kind() == "cascade")
        .and_then(|cascade| cascade.named_child(0))
        .map(|target| receiver_text(target, src))
        .unwrap_or_default();
    let name = text(property, src).trim().to_string();
    push_site(out, name, Some(format!(".{target}")), RefKind::Calls, property, scope);
}

/// `new Foo()`, `const Foo.named()`, `Foo<int>.named()`: a call of the type, or of the named
/// constructor with the type as its receiver.
fn construction(node: Node, src: &str, out: &mut Sink, scope: usize) {
    let Some(ty) = node.child_by_field_name("type") else {
        return;
    };
    let type_name = match ty.kind() {
        "type" => {
            // The call is the site; the same type is no second, plain reference.
            out.claimed.insert(ty.id());
            type_parts(ty, src)
        }
        _ => type_name_parts(node, src),
    };
    let Some((name, prefix)) = type_name else {
        return;
    };
    match node.child_by_field_name("constructor") {
        Some(ctor) => {
            let ctor_name = text(ctor, src).trim().to_string();
            let owner = match prefix {
                Some(prefix) => format!("{prefix}.{name}"),
                None => name,
            };
            push_site(out, ctor_name, Some(format!(".{owner}")), RefKind::Calls, ctor, scope);
        }
        // `new Foo.named()` reads as the type `Foo.named`: a member call of what precedes the dot,
        // exactly as the implicit-new `Foo.named()` (or `m.Foo()`) is.
        None => push_site(
            out,
            name,
            prefix.map(|prefix| format!(".{prefix}")),
            RefKind::Calls,
            ty,
            scope,
        ),
    }
}

/// The type of a `constructor_invocation`, whose `type` field is bare identifiers.
fn type_name_parts(node: Node, src: &str) -> Option<(String, Option<String>)> {
    let mut cursor = node.walk();
    let idents: Vec<String> = node
        .children_by_field_name("type", &mut cursor)
        .map(|n| text(n, src).trim().to_string())
        .collect();
    let name = idents.last()?.clone();
    let prefix = (idents.len() > 1).then(|| idents[0].clone());
    Some((name, prefix))
}

/// `(name, import prefix)` of a `type` node: its last identifier and the one before it.
fn type_parts(node: Node, src: &str) -> Option<(String, Option<String>)> {
    let idents: Vec<String> = kids(node)
        .into_iter()
        .filter(|n| n.kind() == "type_identifier")
        .map(|n| text(n, src).trim().to_string())
        .collect();
    let name = idents.last()?.clone();
    if name.is_empty() || PRIMITIVES.contains(&name.as_str()) {
        return None;
    }
    let prefix = (idents.len() > 1).then(|| idents[0].clone());
    Some((name, prefix))
}

fn type_site(node: Node, src: &str, out: &mut Sink, scope: usize, kind: RefKind) {
    if let Some((name, prefix)) = type_parts(node, src) {
        push_site(out, name, prefix, kind, node, scope);
    }
}

/// `extends`, `with`, `implements`, `on`: base types of a declaration.
fn heritage(node: Node, src: &str, out: &mut Sink, scope: usize) {
    for child in kids(node) {
        match child.kind() {
            "superclass" => {
                for part in kids(child) {
                    match part.kind() {
                        "type" => base_type(part, src, out, scope, RefKind::Inherits),
                        "mixins" => base_types(part, src, out, scope, RefKind::Implements),
                        _ => {}
                    }
                }
            }
            "mixins" | "interfaces" => base_types(child, src, out, scope, RefKind::Implements),
            // The `on` types of a mixin.
            "type" if node.kind() == "mixin_declaration" => base_type(child, src, out, scope, RefKind::Inherits),
            "mixin_application_class" => {
                for app in kids(child).into_iter().filter(|n| n.kind() == "mixin_application") {
                    for part in kids(app) {
                        match part.kind() {
                            "type" => base_type(part, src, out, scope, RefKind::Inherits),
                            "mixins" | "interfaces" => base_types(part, src, out, scope, RefKind::Implements),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn base_types(list: Node, src: &str, out: &mut Sink, scope: usize, kind: RefKind) {
    for ty in kids(list).into_iter().filter(|n| n.kind() == "type") {
        base_type(ty, src, out, scope, kind);
    }
}

fn base_type(ty: Node, src: &str, out: &mut Sink, scope: usize, kind: RefKind) {
    out.claimed.insert(ty.id());
    type_site(ty, src, out, scope, kind);
}

/// `Colors.red` and `Status.active` without a call: the capitalised receiver is a type.
fn static_receiver(node: Node, src: &str, out: &mut Sink, scope: usize) {
    let called = node.parent().is_some_and(|parent| {
        parent.kind() == "call_expression"
            && parent
                .child_by_field_name("function")
                .is_some_and(|function| function.id() == node.id())
    });
    if called {
        return;
    }
    if let Some(object) = node.child_by_field_name("object")
        && object.kind() == "identifier"
    {
        let name = text(object, src).trim().to_string();
        if name.starts_with(|c: char| c.is_uppercase()) {
            push_site(out, name, None, RefKind::References, object, scope);
        }
    }
}

fn annotation(node: Node, src: &str, out: &mut Sink, scope: usize) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    let full = collapse(text(name, src));
    let (qualifier, last) = match full.rsplit_once('.') {
        Some((qualifier, last)) => (Some(qualifier.to_string()), last.to_string()),
        None => (None, full.clone()),
    };
    if last != "override" {
        push_site(out, last, qualifier, RefKind::References, name, scope);
    }
}

/// A bare name given to a call: a provider, a tear-off, a constant.
fn bare_arguments(node: Node, src: &str, out: &mut Sink, scope: usize) {
    for child in kids(node) {
        if child.kind() == "identifier" {
            let name = text(child, src).trim().to_string();
            if name != "_" {
                push_site(out, name, None, RefKind::References, child, scope);
            }
        }
    }
}

// ---------------------------------------------------------------- directives

fn directive(node: Node, src: &str, out: &mut Sink) {
    let line = line_of(node.start_position().row);
    let (word, uri_holder) = match node.kind() {
        "library_import" => ("import", child_of(node, "import_specification")),
        "library_export" => ("export", Some(node)),
        "part_directive" => ("part", Some(node)),
        _ => ("part-of", Some(node)),
    };
    let Some(holder) = uri_holder else {
        return;
    };
    let uri = match node.kind() {
        "part_of_directive" => child_of(node, "uri")
            .and_then(|uri| uri_text(uri, src))
            .or_else(|| child_of(node, "dotted_identifier_list").map(|n| collapse(text(n, src)).replace(' ', ""))),
        _ => holder
            .child_by_field_name("uri")
            .and_then(|uri| uri_text(first_uri(uri), src)),
    };
    let Some(uri) = uri else {
        return;
    };
    if uri.is_empty() {
        return;
    }
    let mut words = vec![word.to_string()];
    let text_of = |n: Node| text(n, src).trim().to_string();
    let prefix = holder.child_by_field_name("alias").map(text_of);
    let mut cursor = holder.walk();
    if holder.children(&mut cursor).any(|n| n.kind() == "deferred") {
        words.push("deferred".to_string());
    }
    if let Some(prefix) = &prefix {
        words.push(format!("as={prefix}"));
    }
    let mut shows = false;
    for combinator in kids(holder).into_iter().filter(|n| n.kind() == "combinator") {
        let names: Vec<String> = kids(combinator).into_iter().map(text_of).collect();
        let key = if text(combinator, src).trim_start().starts_with("show") {
            shows = true;
            "show"
        } else {
            "hide"
        };
        words.push(format!("{key}={}", names.join(",")));
    }
    if node.kind() == "library_import" {
        out.imports.push(ImportDecl {
            path: uri.clone(),
            alias: prefix,
            glob: !shows,
            is_static: false,
            global: false,
            line,
        });
    }
    out.refs.push(RefSite {
        name: uri,
        qualifier: Some(words.join(" ")),
        kind: RefKind::Imports,
        line,
        scope: Some(0),
    });
}

/// The first `uri` of a `configurable_uri` (the rest are conditional alternatives).
fn first_uri(node: Node) -> Node {
    if node.kind() == "configurable_uri" {
        child_of(node, "uri").unwrap_or(node)
    } else {
        node
    }
}

/// The text of a URI string literal without its quotes.
fn uri_text(node: Node, src: &str) -> Option<String> {
    let raw = text(node, src).trim();
    let raw = raw
        .strip_prefix('r')
        .filter(|rest| rest.starts_with(['\'', '"']))
        .unwrap_or(raw);
    for quote in ["'''", "\"\"\"", "'", "\""] {
        if let Some(inner) = raw.strip_prefix(quote) {
            let inner = inner.split(quote).next().unwrap_or(inner);
            return Some(inner.trim().to_string());
        }
    }
    None
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
