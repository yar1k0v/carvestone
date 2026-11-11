//! Which git repo holds a path. A product root may be one repo, a plain folder
//! holding several (`stone/`, `api/`, `portal/`), or a mix; paths stay
//! root-relative with `/` everywhere and this maps them to a repo and back.
//!
//! One code path serves every layout: a one-repo root is a `Repos` with one entry
//! whose directory is the root itself.

use std::path::Path;

use carvestone_git::Git;

/// One git repo at or below the product root.
#[derive(Debug, Clone)]
pub struct RepoRef {
    dir: String,
    git: Git,
}

impl RepoRef {
    /// The repo folder relative to the root: `""` for the root, else `api` (no slashes at the ends).
    pub fn dir(&self) -> &str {
        &self.dir
    }

    /// Git for this repo; every call runs inside its folder.
    pub fn git(&self) -> &Git {
        &self.git
    }
}

/// The git repos found at or below a product root, as of one `discover` call.
#[derive(Debug, Clone, Default)]
pub struct Repos {
    /// Shallowest first, so the root repo leads and the order is stable.
    repos: Vec<RepoRef>,
}

impl Repos {
    /// Looks for `.git` (folder or file, so submodules and worktrees count) in the root, in
    /// `stone/` and its direct subfolders, and in each of `dirs` (the code and doc roots,
    /// root-relative). No deeper walk; a folder outside the root is ignored. A root with no
    /// `.git` of its own inside a bigger repo gets that repo as its root repo.
    pub fn discover(root: &Path, dirs: &[String]) -> Repos {
        let mut candidates = vec![String::new(), "stone".to_string()];
        if let Ok(entries) = std::fs::read_dir(root.join("stone")) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    candidates.push(format!("stone/{}", entry.file_name().to_string_lossy()));
                }
            }
        }
        candidates.extend(dirs.iter().cloned());
        let mut repos: Vec<RepoRef> = Vec::new();
        for candidate in candidates {
            let Some(dir) = normalize(&candidate) else {
                continue;
            };
            if repos.iter().any(|r| r.dir == dir) {
                continue;
            }
            let path = if dir.is_empty() {
                root.to_path_buf()
            } else {
                root.join(&dir)
            };
            let is_root = dir.is_empty();
            if path.join(".git").exists() || (is_root && ancestor_repo(root)) {
                repos.push(RepoRef {
                    git: Git::new(&path),
                    dir,
                });
            }
        }
        repos.sort_by(|a, b| a.dir.len().cmp(&b.dir.len()).then_with(|| a.dir.cmp(&b.dir)));
        Repos { repos }
    }

    /// Every repo found, shallowest first.
    pub fn all(&self) -> &[RepoRef] {
        &self.repos
    }

    /// The deepest repo containing `root_rel` and the path inside it (`""` for the repo folder
    /// itself). `None` when no repo holds the path, or the path leaves the root.
    pub fn repo_of(&self, root_rel: &str) -> Option<(&RepoRef, String)> {
        let path = normalize(root_rel)?;
        self.repos
            .iter()
            .filter(|r| contains(&r.dir, &path))
            .max_by_key(|r| r.dir.len())
            .map(|r| {
                let inner = if r.dir.is_empty() {
                    path.clone()
                } else {
                    path[r.dir.len()..].trim_start_matches('/').to_string()
                };
                (r, inner)
            })
    }

    /// The repos a listing of the folder `root_rel` must ask: the ones holding it and the ones
    /// inside it. Empty when the folder is in no repo and has none below it.
    pub fn around(&self, root_rel: &str) -> Vec<&RepoRef> {
        let Some(dir) = normalize(root_rel) else {
            return Vec::new();
        };
        self.repos
            .iter()
            .filter(|r| contains(&r.dir, &dir) || contains(&dir, &r.dir))
            .collect()
    }

    /// The root-relative path of `repo_rel` inside `repo`.
    pub fn to_root(&self, repo: &RepoRef, repo_rel: &str) -> String {
        let inner = repo_rel.trim_start_matches('/');
        match (repo.dir.is_empty(), inner.is_empty()) {
            (true, _) => inner.to_string(),
            (false, true) => repo.dir.clone(),
            (false, false) => format!("{}/{inner}", repo.dir),
        }
    }

    /// The repo that holds `stone/`, where items, ids and their history live.
    pub fn stone_repo(&self) -> Option<&RepoRef> {
        self.repo_of("stone").map(|(repo, _)| repo)
    }
}

/// Whether a folder above `root` is a git repo. Git run from inside the root then sees the root's
/// own files only, with paths relative to it, so the ancestor repo stands in for a root repo.
fn ancestor_repo(root: &Path) -> bool {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    root.ancestors().skip(1).any(|dir| dir.join(".git").exists())
}

/// Whether git tracks the root: it is a repo itself or sits inside one.
pub(crate) fn inside_repo(root: &Path) -> bool {
    root.join(".git").exists() || ancestor_repo(root)
}

/// Whether `path` is `dir` or below it; both normalized.
fn contains(dir: &str, path: &str) -> bool {
    dir.is_empty() || path == dir || path.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/'))
}

/// `a\b/./c/` as `a/b/c`; `""` for the root; `None` for an absolute path or one that climbs out.
fn normalize(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}
