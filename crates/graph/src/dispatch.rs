//! Interface and abstract dispatch. A call that resolves to an interface or
//! abstract member runs one of its implementers' members, so each such member
//! gets one inferred `dispatch` edge to the same-named member of every type
//! that implements or extends its type. The edge is per member pair, never per
//! call site, and it is derived from the exact `inherits`/`implements` edges
//! alone: a base the resolver could not pin to one type dispatches nowhere.
//!
//! What counts as dispatchable, by language:
//! - C#, TypeScript: every member of an `interface`; the `abstract` members of
//!   an abstract class;
//! - Rust: the methods of a `trait`;
//! - Python: the public methods of a class that has `ABC` or `Protocol` in its
//!   base list;
//! - Dart: every method of an `interface class`; the body-less methods (and getters) of an
//!   abstract or sealed class or a mixin.
//!
//! An implementer with several members of the base member's name pairs by
//! parameter count only, and is skipped when that does not single one out.

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{DeclKind, Lang, RefKind};
use crate::resolve::{Edge, Node, SymRef, World};
use crate::resolve_csharp::file_projects;

/// Dispatch edges for the `inherits`/`implements` edges in `edges`, each with
/// the index of the file that edge sits in. A dispatch edge belongs to that
/// file, so re-resolving the file rewrites it and removing the file drops it.
/// The result is a function of the world and of those edges only.
pub fn dispatch(world: &World, edges: &[Edge]) -> Vec<(usize, Edge)> {
    let mut pairs: BTreeSet<(usize, SymRef, SymRef)> = BTreeSet::new();
    for edge in edges {
        if !matches!(edge.kind, RefKind::Inherits | RefKind::Implements) || !edge.exact {
            continue;
        }
        let (Node::Decl(site), Some(Node::Decl(base))) = (&edge.src, &edge.dst) else {
            continue;
        };
        // A base list on the same line as a member is scoped to the member.
        let Some(derived) = enclosing_type(world, *site) else {
            continue;
        };
        let (d, b) = (world.decl(derived), world.decl(*base));
        if b.kind == DeclKind::Type && d.qname != b.qname {
            pairs.insert((derived.file, derived, *base));
        }
    }
    if pairs.is_empty() {
        return Vec::new();
    }

    let types = Types::new(
        world,
        pairs.iter().any(|(_, d, _)| world.files()[d.file].lang == Lang::CSharp),
    );
    let mut bases: BTreeMap<SymRef, Vec<SymRef>> = BTreeMap::new();
    let mut found: BTreeSet<(usize, SymRef, SymRef)> = BTreeSet::new();
    for (owner, derived, base) in pairs {
        let from = bases.entry(base).or_insert_with(|| types.dispatchable(base)).clone();
        if from.is_empty() {
            continue;
        }
        let lang = world.files()[derived.file].lang;
        let own = types.type_name(derived);
        let mut by_name: BTreeMap<&str, Vec<SymRef>> = BTreeMap::new();
        for member in types.members(derived) {
            let decl = world.decl(member);
            let method_only = matches!(lang, Lang::Rust | Lang::Python);
            if (method_only && decl.kind != DeclKind::Method) || (decl.kind == DeclKind::Method && decl.name == own) {
                continue;
            }
            by_name.entry(decl.name.as_str()).or_default().push(member);
        }
        for member in &from {
            let name = world.decl(*member).name.as_str();
            let Some(candidates) = by_name.get(name) else {
                continue;
            };
            let overloaded = from.iter().filter(|m| world.decl(**m).name == name).count() > 1;
            let chosen = match candidates.as_slice() {
                [only] if !overloaded => Some(*only),
                many => {
                    let want = param_count(&world.decl(*member).signature, name);
                    let mut same = many
                        .iter()
                        .filter(|c| param_count(&world.decl(**c).signature, name) == want);
                    match (same.next(), same.next()) {
                        (Some(one), None) => Some(*one),
                        _ => None,
                    }
                }
            };
            if let Some(chosen) = chosen {
                found.insert((owner, *member, chosen));
            }
        }
    }
    found
        .into_iter()
        .map(|(owner, src, dst)| {
            (
                owner,
                Edge {
                    src: Node::Decl(src),
                    dst: Some(Node::Decl(dst)),
                    dst_name: world.decl(dst).qname.clone(),
                    kind: RefKind::Dispatch,
                    exact: false,
                    // The line the call lands on: the implementer's member, not the interface's.
                    line: world.decl(dst).line_start,
                },
            )
        })
        .collect()
}

fn enclosing_type(world: &World, from: SymRef) -> Option<SymRef> {
    let mut current = from;
    loop {
        let decl = world.decl(current);
        if decl.kind == DeclKind::Type {
            return Some(current);
        }
        current = SymRef {
            file: current.file,
            decl: decl.parent?,
        };
    }
}

/// The parts of a type: C# partial declarations share one qualified name and
/// together hold the members, every other language keeps a type in one place.
/// A C# type is its project plus its qualified name, as in the resolver.
struct Types<'w> {
    world: &'w World,
    file_project: Vec<Option<usize>>,
    csharp: BTreeMap<(Option<usize>, &'w str), Vec<SymRef>>,
}

impl<'w> Types<'w> {
    fn new(world: &'w World, any_csharp: bool) -> Self {
        let mut csharp: BTreeMap<(Option<usize>, &str), Vec<SymRef>> = BTreeMap::new();
        let mut file_project = Vec::new();
        if any_csharp {
            file_project = file_projects(world);
            for (file, facts) in world.files().iter().enumerate() {
                if facts.lang != Lang::CSharp {
                    continue;
                }
                for (decl, d) in facts.decls.iter().enumerate() {
                    if d.kind == DeclKind::Type {
                        csharp
                            .entry((file_project[file], d.qname.as_str()))
                            .or_default()
                            .push(SymRef { file, decl });
                    }
                }
            }
        }
        Self {
            world,
            file_project,
            csharp,
        }
    }

    fn parts(&self, ty: SymRef) -> Vec<SymRef> {
        if self.world.files()[ty.file].lang == Lang::CSharp
            && let Some(all) = self
                .csharp
                .get(&(self.file_project[ty.file], self.world.decl(ty).qname.as_str()))
        {
            return all.clone();
        }
        vec![ty]
    }

    fn type_name(&self, ty: SymRef) -> &'w str {
        self.world.decl(ty).name.as_str()
    }

    /// Methods and members declared directly in the type, over all its parts.
    fn members(&self, ty: SymRef) -> Vec<SymRef> {
        let mut out = Vec::new();
        for part in self.parts(ty) {
            let facts = &self.world.files()[part.file];
            for (decl, d) in facts.decls.iter().enumerate() {
                if d.parent == Some(part.decl) && matches!(d.kind, DeclKind::Method | DeclKind::Member) {
                    out.push(SymRef { file: part.file, decl });
                }
            }
        }
        out
    }

    /// The members of `base` that a call may reach through an implementer.
    fn dispatchable(&self, base: SymRef) -> Vec<SymRef> {
        let lang = self.world.files()[base.file].lang;
        let parts = self.parts(base);
        let has = |word: &str| {
            parts
                .iter()
                .any(|p| words(&self.world.decl(*p).signature).any(|w| w == word))
        };
        let members = self.members(base);
        let keep = |filter: &dyn Fn(SymRef) -> bool| -> Vec<SymRef> {
            members.iter().copied().filter(|m| filter(*m)).collect()
        };
        match lang {
            Lang::CSharp | Lang::TypeScript | Lang::JavaScript => {
                if has("interface") {
                    members.clone()
                } else if has("abstract") {
                    keep(&|m| words(&self.world.decl(m).signature).any(|w| w == "abstract"))
                } else {
                    Vec::new()
                }
            }
            Lang::Rust if has("trait") => keep(&|m| self.world.decl(m).kind == DeclKind::Method),
            Lang::Python if has("ABC") || has("ABCMeta") || has("Protocol") => keep(&|m| {
                let decl = self.world.decl(m);
                decl.kind == DeclKind::Method && !decl.name.starts_with('_')
            }),
            // An `interface class` is all contract; an abstract (or sealed) class or a mixin
            // promises the members it leaves without a body.
            Lang::Dart => {
                let own = self.world.decl(base).name.as_str();
                let methods = |m: &SymRef| {
                    let decl = self.world.decl(*m);
                    decl.kind == DeclKind::Method && decl.name != "new" && !is_constructor(&decl.signature, own)
                };
                if has("interface") {
                    keep(&|m| methods(&m))
                } else if has("abstract") || has("sealed") || has("mixin") {
                    keep(&|m| methods(&m) && words(&self.world.decl(m).signature).any(|w| w == "abstract"))
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }
}

/// Whether a Dart signature declares a constructor of type `own`: `Own(..)`, `const Own(..)`,
/// `Own.named(..)`, `factory Own.make(..)`.
fn is_constructor(signature: &str, own: &str) -> bool {
    let mut rest = signature;
    while let Some((word, tail)) = rest.split_once(' ') {
        if matches!(word, "const" | "factory" | "external") {
            rest = tail;
        } else {
            break;
        }
    }
    rest.strip_prefix(own)
        .is_some_and(|after| after.starts_with('(') || after.starts_with('.'))
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
}

/// The number of parameters in the first `name(...)` of a signature, or
/// `None` for a member without a parameter list (a property, a field).
fn param_count(signature: &str, name: &str) -> Option<usize> {
    let bytes = signature.as_bytes();
    let mut from = 0;
    while let Some(found) = signature[from..].find(name) {
        let start = from + found;
        let end = start + name.len();
        from = end;
        let before_ok = signature[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if !before_ok {
            continue;
        }
        let mut at = end;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'<') {
            at = skip_generics(signature, at)?;
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
        }
        if bytes.get(at) == Some(&b'(') {
            return Some(count_params(&signature[at + 1..]));
        }
    }
    None
}

/// The index after the `>` closing the `<` at `open`.
fn skip_generics(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut previous = ' ';
    for (i, c) in text[open..].char_indices() {
        match c {
            '<' => depth += 1,
            '>' if previous != '=' && previous != '-' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + i + 1);
                }
            }
            _ => {}
        }
        previous = c;
    }
    None
}

/// Top-level parameters before the `)` that closes the list `rest` starts in.
fn count_params(rest: &str) -> usize {
    let mut depth = 0usize;
    let mut previous = ' ';
    let mut count = 0;
    let mut filled = false;
    for c in rest.chars() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => depth -= 1,
            // `=>` and `->` are not closing brackets.
            '>' if previous != '=' && previous != '-' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                count += 1;
                filled = false;
                previous = c;
                continue;
            }
            _ => {}
        }
        if !c.is_whitespace() {
            filled = true;
        }
        previous = c;
    }
    count + usize::from(filled)
}
