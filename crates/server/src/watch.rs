//! The file watcher: `notify` over `stone/`, the code roots and the doc roots, debounced, feeding the event bus.
//!
//! Events are built from the paths of a batch, not from what the refresh changed: another
//! process may have indexed first, and a deleted file has nothing left to compare. The watcher
//! only refreshes the derived index; it never takes the store lock.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use carvestone_engine::Repo;
use carvestone_engine::api::{CodeEvent, ErrorEvent, ItemsEvent};
use carvestone_engine::changes::{code_paths, ids_for_paths, is_doc_file, repo_relative};
use carvestone_engine::query::etag_of;
use carvestone_engine::refresh::refresh;
use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::ServerError;
use crate::events::{Bus, Change};

/// Saves closer together than this are one batch.
const QUIET: Duration = Duration::from_millis(200);
/// A batch is closed after this long even when changes keep coming.
const MAX_BATCH: Duration = Duration::from_secs(2);
/// Folders no change inside which matters.
const IGNORED: [&str; 4] = [".git", ".stone", "target", "node_modules"];

/// Keeps the watcher running; dropping it stops the watcher and its thread.
pub struct Watcher {
    _watcher: RecommendedWatcher,
}

enum Msg {
    Path(PathBuf),
    Failed(String),
}

/// `web/dist` and the folders in `IGNORED`, anywhere in a repo-relative path.
fn ignored(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    parts.iter().any(|part| IGNORED.contains(part)) || parts.windows(2).any(|pair| pair == ["web", "dist"])
}

/// The folders to watch: `stone/`, each code root and each doc root, minus any inside another.
fn roots(repo: &Repo) -> Vec<PathBuf> {
    let base = &repo.store.layout.root;
    let config = &repo.store.config;
    let mut roots = vec![repo.store.layout.data_dir.clone()];
    for root in config.code_roots.iter().chain(&config.doc_roots) {
        let root = root.trim().trim_start_matches("./").trim_end_matches('/');
        roots.push(if root.is_empty() || root == "." {
            base.clone()
        } else {
            base.join(root)
        });
    }
    roots.sort_by_key(|root| root.components().count());
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if root.is_dir() && !kept.iter().any(|outer| root.starts_with(outer)) {
            kept.push(root);
        }
    }
    kept
}

pub(crate) fn start(repo: Repo, bus: Bus) -> Result<Watcher, ServerError> {
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
        // Linux reports a finished write as `Access(Close(Write))`; every other access (open,
        // read) changes nothing.
        Ok(event) if matches!(event.kind, EventKind::Access(kind) if kind != AccessKind::Close(AccessMode::Write)) => {}
        Ok(event) => {
            for path in event.paths {
                let _ = tx.send(Msg::Path(path));
            }
        }
        Err(err) => {
            let _ = tx.send(Msg::Failed(err.to_string()));
        }
    })?;
    let watched = roots(&repo);
    for root in &watched {
        watcher.watch(root, RecursiveMode::Recursive)?;
    }
    // The root's own README, CLAUDE.md and AGENTS.md, when no root above covers it.
    let base = &repo.store.layout.root;
    if !watched.iter().any(|root| base.starts_with(root)) {
        watcher.watch(base, RecursiveMode::NonRecursive)?;
    }
    std::thread::Builder::new()
        .name("stone-watch".to_string())
        .spawn(move || debounce(&repo, &bus, &rx))
        .map_err(ServerError::Io)?;
    Ok(Watcher { _watcher: watcher })
}

/// Collects messages until the folder is quiet for `QUIET`, handles the batch, repeats. Ends when
/// the watcher (the sender) is dropped.
fn debounce(repo: &Repo, bus: &Bus, rx: &mpsc::Receiver<Msg>) {
    while let Ok(first) = rx.recv() {
        let started = Instant::now();
        let mut batch = Batch::default();
        batch.add(first);
        loop {
            let left = MAX_BATCH.saturating_sub(started.elapsed()).min(QUIET);
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(msg) => batch.add(msg),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    batch.handle(repo, bus);
                    return;
                }
            }
        }
        batch.handle(repo, bus);
    }
}

#[derive(Default)]
struct Batch {
    paths: BTreeSet<PathBuf>,
    failure: Option<String>,
}

impl Batch {
    fn add(&mut self, msg: Msg) {
        match msg {
            Msg::Path(path) => {
                self.paths.insert(path);
            }
            Msg::Failed(err) => self.failure = Some(err),
        }
    }

    /// Refreshes the index, then tells the clients what the paths of the batch were.
    fn handle(self, repo: &Repo, bus: &Bus) {
        // Item files and doc-root files both announce items; only the store's own are ones the
        // server may have just written. A doc-root file inside a code root is both an item for
        // the clients and a code file, so it goes down both paths.
        let in_stone = |path: &PathBuf| repo_relative(repo, path).is_some_and(|rel| rel.starts_with("stone/"));
        let items: Vec<&PathBuf> = self
            .paths
            .iter()
            .filter(|path| in_stone(path) || repo_relative(repo, path).is_some_and(|rel| is_doc_file(repo, &rel)))
            .filter(|path| !written_here(repo, bus, path))
            .collect();
        let ids = ids_for_paths(repo, &items);
        let others: Vec<PathBuf> = self.paths.iter().filter(|path| !in_stone(path)).cloned().collect();
        let mut code = code_paths(repo, &watched_files(repo, &others));
        // A `.git` entry that appears or goes away is a repo that came or left; its history and
        // file listings change although no watched file did.
        let repos = repos_moved(repo, &self.paths);
        if ids.is_empty() && code.is_empty() && repos.is_empty() && self.failure.is_none() {
            return;
        }
        if let Some(gate) = repo.freshness() {
            gate.invalidate();
        }
        if let Err(err) = refresh(repo) {
            bus.publish(Change::Error(ErrorEvent { error: err.to_string() }));
            return;
        }
        if !ids.is_empty() {
            bus.publish(Change::Items(ItemsEvent { ids }));
        }
        code.extend(repos);
        code.sort();
        code.dedup();
        if !code.is_empty() {
            bus.publish(Change::Code(CodeEvent { paths: code }));
        }
        if let Some(err) = self.failure {
            bus.publish(Change::Error(ErrorEvent {
                error: format!("the file watcher lost track of changes ({err}); reload the page"),
            }));
        }
    }
}

/// The root-relative folders (`.` for the root itself) whose `.git` entry itself is among
/// `paths`: a repo was created, cloned or removed there. Changes inside `.git` do not count.
fn repos_moved(repo: &Repo, paths: &BTreeSet<PathBuf>) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|path| repo_relative(repo, path))
        .filter_map(|rel| {
            let folder = if rel == ".git" {
                Some("")
            } else {
                rel.strip_suffix("/.git")
            }?;
            Some(if folder.is_empty() {
                ".".to_string()
            } else {
                folder.to_string()
            })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// True when `path` holds the bytes a write route just saved: that route already told the clients.
fn written_here(repo: &Repo, bus: &Bus, path: &Path) -> bool {
    let (Some(rel), Ok(text)) = (repo_relative(repo, path), std::fs::read_to_string(path)) else {
        return false;
    };
    bus.take_own_write(&rel, &etag_of(&text))
}

/// Paths outside the ignored folders that are not folders themselves (a folder event says
/// nothing about which file changed).
fn watched_files(repo: &Repo, paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| !path.is_dir())
        .filter(|path| repo_relative(repo, path).is_some_and(|rel| !ignored(&rel)))
        .cloned()
        .collect()
}
