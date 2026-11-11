//! The Rust module tree: crate roots from the Cargo workspace, the files and
//! inline modules reached through `mod` declarations, and the lookups the
//! resolver needs (which module holds a line, which crate a name is).
//!
//! A `#[path]` module and everything below it is `inferred`. A file no `mod`
//! reaches becomes a detached node of the lib crate of its package, also
//! inferred.

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{DeclKind, Lang};
use crate::project::{ProjectFacts, ProjectKind};
use crate::resolve::{SymRef, World};

pub(crate) struct ModNode {
    pub parent: Option<usize>,
    /// Index into [`Tree::crates`].
    pub krate: usize,
    pub file: usize,
    /// The inline `mod x { }` declaration; `None` for a whole file.
    pub decl: Option<usize>,
    /// The declaration edges to this module point at: the `mod x;` or inline
    /// declaration, or the file itself for a crate root.
    pub sym: SymRef,
    pub inferred: bool,
    pub lines: (u32, u32),
}

pub(crate) struct Crate {
    /// Directory of the package the crate belongs to.
    pub dir: String,
    pub root: usize,
    pub lib: bool,
    /// Lib crates this crate may name; `None` when no Cargo.toml says.
    pub visible: Option<BTreeSet<usize>>,
}

#[derive(Default)]
pub(crate) struct Tree {
    pub nodes: Vec<ModNode>,
    pub crates: Vec<Crate>,
    /// Nodes per file, the whole-file node first.
    pub by_file: BTreeMap<usize, Vec<usize>>,
    /// Module declaration to its node.
    pub by_decl: BTreeMap<SymRef, usize>,
    /// Lib crate by import name.
    pub lib_by_name: BTreeMap<String, usize>,
}

impl Tree {
    /// The innermost module of `file` holding `line`.
    pub fn node_at(&self, file: usize, line: u32) -> Option<usize> {
        let nodes = self.by_file.get(&file)?;
        let inline = nodes
            .iter()
            .copied()
            .filter(|&n| self.nodes[n].decl.is_some() && self.nodes[n].lines.0 <= line && line <= self.nodes[n].lines.1)
            .min_by_key(|&n| self.nodes[n].lines.1 - self.nodes[n].lines.0);
        inline.or_else(|| nodes.first().copied())
    }

    pub fn crate_of_file(&self, file: usize) -> Option<usize> {
        self.by_file.get(&file)?.first().map(|&n| self.nodes[n].krate)
    }
}

pub(crate) fn build(world: &World) -> Tree {
    let mut builder = Builder {
        world,
        tree: Tree::default(),
        seen: BTreeSet::new(),
    };
    let rust: Vec<usize> = (0..world.files().len())
        .filter(|&index| world.files()[index].lang == Lang::Rust)
        .collect();
    for &file in &rust {
        if let Some((name, lib, dir)) = root_of(world, file) {
            builder.root(file, name, lib, dir);
        }
    }
    for &file in &rust {
        if !builder.seen.contains(&file) {
            builder.orphan(file);
        }
    }
    builder.tree.link_deps(world);
    builder.tree
}

impl Tree {
    /// Which lib crates each crate can name: the lib of its own package and
    /// the packages its Cargo.toml lists (`-` is `_`). A crate with no
    /// Cargo.toml sees every lib.
    fn link_deps(&mut self, world: &World) {
        let mut visible = Vec::with_capacity(self.crates.len());
        for krate in &self.crates {
            let manifest = path_join(&krate.dir, "Cargo.toml");
            let Some(project) = deepest_cargo(world, &manifest).filter(|project| project.root == krate.dir) else {
                visible.push(None);
                continue;
            };
            let mut set = BTreeSet::new();
            for (other, candidate) in self.crates.iter().enumerate() {
                if candidate.lib && candidate.dir == krate.dir {
                    set.insert(other);
                }
            }
            for package in &project.packages {
                if let Some(&dep) = self.lib_by_name.get(&package.replace('-', "_")) {
                    set.insert(dep);
                }
            }
            visible.push(Some(set));
        }
        for (krate, set) in self.crates.iter_mut().zip(visible) {
            krate.visible = set;
        }
    }

    /// Whether code in crate `from` may name items of crate `to`.
    pub fn sees(&self, from: usize, to: usize) -> bool {
        from == to || self.crates[from].visible.as_ref().is_none_or(|set| set.contains(&to))
    }
}

struct Builder<'w> {
    world: &'w World,
    tree: Tree,
    seen: BTreeSet<usize>,
}

impl Builder<'_> {
    fn root(&mut self, file: usize, name: String, lib: bool, dir: String) {
        let krate = self.tree.crates.len();
        let node = self.tree.nodes.len();
        self.tree.crates.push(Crate {
            dir,
            root: node,
            lib,
            visible: None,
        });
        if lib {
            self.tree.lib_by_name.entry(name).or_insert(krate);
        }
        self.file_node(file, None, krate, SymRef { file, decl: 0 }, false, true);
    }

    fn orphan(&mut self, file: usize) {
        let path = &self.world.files()[file].path;
        let krate = self
            .tree
            .crates
            .iter()
            .enumerate()
            .filter(|(_, krate)| krate.lib && contains(&krate.dir, path))
            .max_by_key(|(_, krate)| krate.dir.len())
            .map(|(index, _)| index);
        let krate = krate.unwrap_or_else(|| {
            self.tree.crates.push(Crate {
                dir: dir_of(path),
                root: self.tree.nodes.len(),
                lib: false,
                visible: None,
            });
            self.tree.crates.len() - 1
        });
        self.file_node(file, None, krate, SymRef { file, decl: 0 }, true, false);
    }

    fn file_node(
        &mut self,
        file: usize,
        parent: Option<usize>,
        krate: usize,
        sym: SymRef,
        inferred: bool,
        root: bool,
    ) -> usize {
        let index = self.tree.nodes.len();
        self.seen.insert(file);
        self.tree.nodes.push(ModNode {
            parent,
            krate,
            file,
            decl: None,
            sym,
            inferred,
            lines: (0, u32::MAX),
        });
        self.tree.by_file.entry(file).or_default().push(index);
        if sym.decl != 0 {
            self.tree.by_decl.insert(sym, index);
        }
        let base = child_dir(&self.world.files()[file].path, root);
        self.populate(index, file, None, &base);
        index
    }

    /// Add the modules declared directly under `scope` of `file`.
    fn populate(&mut self, node: usize, file: usize, scope: Option<usize>, base: &str) {
        let world = self.world;
        let facts = &world.files()[file];
        for (index, decl) in facts.decls.iter().enumerate() {
            let direct = match scope {
                Some(s) => decl.parent == Some(s),
                None => matches!(decl.parent, None | Some(0)),
            };
            if decl.kind != DeclKind::Module || !direct {
                continue;
            }
            let sym = SymRef { file, decl: index };
            let parent = &self.tree.nodes[node];
            let (krate, inferred) = (parent.krate, parent.inferred);
            if !decl.signature.trim_end().ends_with(';') {
                let child = self.tree.nodes.len();
                self.tree.nodes.push(ModNode {
                    parent: Some(node),
                    krate,
                    file,
                    decl: Some(index),
                    sym,
                    inferred,
                    lines: (decl.line_start, decl.line_end),
                });
                self.tree.by_file.entry(file).or_default().push(child);
                self.tree.by_decl.insert(sym, child);
                self.populate(child, file, Some(index), &path_join(base, &decl.name));
                continue;
            }
            let candidates: Vec<(String, bool)> = match path_attr(&decl.signature) {
                Some(path) => vec![(path_join(&dir_of(&facts.path), &path), true)],
                None => vec![
                    (path_join(base, &format!("{}.rs", decl.name)), false),
                    (path_join(base, &format!("{}/mod.rs", decl.name)), false),
                ],
            };
            for (path, via_path) in candidates {
                let Some(target) = world.file_index(&normalize(&path)) else {
                    continue;
                };
                if self.seen.contains(&target) {
                    break;
                }
                self.file_node(target, Some(node), krate, sym, inferred || via_path, false);
                break;
            }
        }
    }
}

/// `(crate name, is lib, package dir)` when `file` is a crate root.
fn root_of(world: &World, file: usize) -> Option<(String, bool, String)> {
    let path = world.files()[file].path.as_str();
    let project = deepest_cargo(world, path);
    let (dir, rel, package) = match project {
        Some(project) => (
            project.root.clone(),
            relative(&project.root, path)?.to_string(),
            project.name.replace('-', "_"),
        ),
        None => {
            let rel = path
                .rsplit_once("/src/")
                .map(|(head, rest)| (head, format!("src/{rest}")));
            match rel {
                Some((head, rel)) => (
                    head.to_string(),
                    rel,
                    head.rsplit('/').next().unwrap_or("crate").replace('-', "_"),
                ),
                None if matches!(path, "lib.rs" | "main.rs") => (String::new(), path.to_string(), "crate".to_string()),
                None => return None,
            }
        }
    };
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["src", "lib.rs"] => Some((package, true, dir)),
        ["src", "main.rs"] | ["build.rs"] => Some((package, false, dir)),
        ["src", "bin", file] | ["tests" | "examples" | "benches", file] if file.ends_with(".rs") => {
            Some((file.trim_end_matches(".rs").to_string(), false, dir))
        }
        ["src", "bin", name, "main.rs"] | ["tests" | "examples" | "benches", name, "main.rs"] => {
            Some(((*name).to_string(), false, dir))
        }
        _ => None,
    }
}

fn deepest_cargo<'w>(world: &'w World, path: &str) -> Option<&'w ProjectFacts> {
    world
        .projects()
        .iter()
        .filter(|project| project.kind == ProjectKind::Cargo && contains(&project.root, path))
        .max_by_key(|project| project.root.len())
}

fn contains(root: &str, path: &str) -> bool {
    root.is_empty() || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

fn relative<'a>(root: &str, path: &'a str) -> Option<&'a str> {
    if root.is_empty() {
        Some(path)
    } else {
        path.strip_prefix(root)?.strip_prefix('/')
    }
}

/// The directory `mod x;` files of a module live in.
fn child_dir(path: &str, root: bool) -> String {
    let dir = dir_of(path);
    let file = path.rsplit('/').next().unwrap_or(path);
    if root || file == "mod.rs" {
        dir
    } else {
        path_join(&dir, &stem(path))
    }
}

fn dir_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

fn stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".rs").unwrap_or(name).to_string()
}

fn path_join(base: &str, rest: &str) -> String {
    if base.is_empty() {
        rest.to_string()
    } else {
        format!("{base}/{rest}")
    }
}

/// Resolve `.` and `..` segments lexically.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The path of a `#[path = "x"] mod x;` signature written by the extractor.
fn path_attr(signature: &str) -> Option<String> {
    let rest = signature.strip_prefix("#[path = \"")?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
