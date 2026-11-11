//! Reading a batch of changed file paths as stone items and code files, for the file watcher.
//!
//! The answer comes from the paths alone, never from the index, so a deleted file and a file
//! another process already indexed are reported like any other.

use std::path::Path;

use carvestone_core::ItemId;

use crate::repo::Repo;

/// `path` relative to the repo root with `/` separators; `None` when it lies outside the repo.
pub fn repo_relative(repo: &Repo, path: &Path) -> Option<String> {
    let rel = if path.is_absolute() {
        path.strip_prefix(&repo.store.layout.root).ok()?
    } else {
        path
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    Some(text.trim_start_matches("./").to_string())
}

/// The ids of the items whose files are among `paths`, each as the path id (`tasks/CS-7`) and,
/// for a numbered item, the short id (`CS-7`). A markdown file under a doc root is an item whose
/// id is its root-relative path (`docs/setup.md`). Sorted, without duplicates.
///
/// Paths may be absolute or relative to the repo root. Anything that is no item file is skipped:
/// other extensions, `index.md` and `log.md` of the store, the file store and paths outside
/// `stone/` and the doc roots.
pub fn ids_for_paths<P: AsRef<Path>>(repo: &Repo, paths: &[P]) -> Vec<String> {
    let scheme = repo.store.config.scheme();
    let mut out = Vec::new();
    for path in paths {
        let Some(rel) = repo_relative(repo, path.as_ref()) else {
            continue;
        };
        if is_doc_file(repo, &rel) {
            out.push(rel);
            continue;
        }
        let Some(inside) = rel.strip_prefix("stone/") else {
            continue;
        };
        let name = inside.rsplit('/').next().unwrap_or(inside);
        if !name.ends_with(".md") || name == "index.md" || name == "log.md" || inside.starts_with("files/") {
            continue;
        }
        let id = ItemId::from_path(inside);
        if let Some(short) = scheme.short(&id) {
            out.push(short);
        }
        out.push(id.as_str().to_string());
    }
    out.sort();
    out.dedup();
    out
}

/// True when `rel` (root-relative) is a markdown file stone indexes under a doc root: the walk
/// skips folders that start with `.` and `node_modules`.
pub fn is_doc_file(repo: &Repo, rel: &str) -> bool {
    let rel = rel.trim_start_matches("./");
    let Some(folder) = repo.store.config.doc_root_of(rel) else {
        return false;
    };
    let inside = rel.get(folder.len()..).unwrap_or_default().trim_start_matches('/');
    let mut parts = inside.split('/').peekable();
    while let Some(part) = parts.next() {
        let is_file = parts.peek().is_none();
        if is_file {
            return part.ends_with(".md");
        }
        if part.starts_with('.') || part == "node_modules" {
            return false;
        }
    }
    false
}

/// The repo-relative paths among `paths` that sit under a configured code root (and not in
/// `stone/`). Sorted, without duplicates.
pub fn code_paths<P: AsRef<Path>>(repo: &Repo, paths: &[P]) -> Vec<String> {
    let prefixes: Vec<String> = repo
        .store
        .config
        .code_roots
        .iter()
        .map(|root| {
            let root = root.trim().trim_start_matches("./").trim_end_matches('/');
            if root.is_empty() || root == "." {
                String::new()
            } else {
                format!("{root}/")
            }
        })
        .collect();
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|path| repo_relative(repo, path.as_ref()))
        .filter(|rel| !rel.starts_with("stone/") && prefixes.iter().any(|prefix| rel.starts_with(prefix.as_str())))
        .collect();
    out.sort();
    out.dedup();
    out
}
