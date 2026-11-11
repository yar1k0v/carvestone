//! The engine's entry point: a repo handle and its snapshot.

use std::path::Path;
use std::sync::Arc;

use carvestone_index::{Index, IndexError};
use carvestone_store::{FrontmatterError, Store, StoreError};

use crate::fresh::Freshness;
use crate::repos::Repos;
use crate::resolve::Snapshot;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Core(#[from] carvestone_core::Error),
    #[error(transparent)]
    Frontmatter(#[from] FrontmatterError),
    #[error("{id} not found{}", suggestion.as_ref().map(|s| format!(". Did you mean {s}?")).unwrap_or_default())]
    NotFound { id: String, suggestion: Option<String> },
    #[error("`{input}` matches {}; keep one file or give one a new number with `stone renumber`", paths.join(", "))]
    Ambiguous { input: String, paths: Vec<String> },
    #[error("{id} has no section #{anchor}{}", sections_hint(anchors))]
    NoSection {
        id: String,
        anchor: String,
        anchors: Vec<String>,
    },
    /// A name that is no item but points at what to run or read instead.
    #[error("{0}")]
    Redirect(String),
    #[error("{id} is not a task")]
    NotATask { id: String },
    #[error("{id} has no ## Outcome; add one with `stone edit {id} --section outcome`")]
    NoOutcome { id: String },
    #[error("{0}")]
    Invalid(String),
    /// The file changed since the caller read it, so the write was refused.
    #[error("{0}")]
    Conflict(String),
    /// A usage error: the CLI exits 2 for it.
    #[error("query has no words; give at least one search term")]
    EmptyQuery,
    #[error(transparent)]
    Index(#[from] carvestone_index::IndexError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    /// The git CLI is missing, too old, timed out or failed: a problem of the machine, not the request.
    #[error(transparent)]
    Git(#[from] carvestone_git::GitError),
}

/// How an adapter should treat an error: the HTTP server maps each kind to a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    Invalid,
    Conflict,
    BadRequest,
    Forbidden,
    Internal,
}

impl ErrorKind {
    /// The name used in JSON error bodies.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::NotFound => "not_found",
            ErrorKind::Invalid => "invalid",
            ErrorKind::Conflict => "conflict",
            ErrorKind::BadRequest => "bad_request",
            ErrorKind::Forbidden => "forbidden",
            ErrorKind::Internal => "internal",
        }
    }
}

impl EngineError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            EngineError::NotFound { .. } | EngineError::NoSection { .. } | EngineError::Redirect(_) => {
                ErrorKind::NotFound
            }
            EngineError::Core(carvestone_core::Error::NotFound { .. }) => ErrorKind::NotFound,
            // A held lock clears by itself: the caller retries instead of seeing a server fault.
            EngineError::Conflict(_)
            | EngineError::Store(StoreError::Exists(_) | StoreError::Busy(_))
            | EngineError::Index(IndexError::RebuildBusy { .. }) => ErrorKind::Conflict,
            EngineError::Sql(err) | EngineError::Index(IndexError::Sqlite(err)) if is_busy(err) => ErrorKind::Conflict,
            EngineError::EmptyQuery => ErrorKind::BadRequest,
            EngineError::Ambiguous { .. }
            | EngineError::NotATask { .. }
            | EngineError::NoOutcome { .. }
            | EngineError::Invalid(_)
            | EngineError::Frontmatter(_)
            | EngineError::Core(_) => ErrorKind::Invalid,
            EngineError::Store(_) | EngineError::Index(_) | EngineError::Sql(_) | EngineError::Git(_) => {
                ErrorKind::Internal
            }
        }
    }
}

/// SQLite gave up waiting for another connection's lock.
fn is_busy(err: &rusqlite::Error) -> bool {
    matches!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

/// Whether `.stone/` accepts a new file: the open failure is a permissions
/// problem, not a damaged index.
fn can_write(index: &Path) -> bool {
    let Some(dir) = index.parent() else {
        return true;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!("write-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// "; sections: a, b" or "; it has no sections".
fn sections_hint(anchors: &[String]) -> String {
    if anchors.is_empty() {
        "; it has no sections".to_string()
    } else {
        format!("; sections: {}", anchors.join(", "))
    }
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub store: Store,
    /// The actor writes are stamped with; `None` reads `STONE_AGENT` or the git user.
    author: Option<String>,
    /// Set by a long-running server, so reads share refreshes; `None` refreshes on every read.
    freshness: Option<Arc<Freshness>>,
}

impl Repo {
    pub fn open(start: &Path) -> Result<Repo, EngineError> {
        Ok(Repo {
            store: Store::open(start)?,
            author: None,
            freshness: None,
        })
    }

    /// The same repo, with reads sharing refreshes through `freshness`.
    pub fn with_freshness(mut self, freshness: Arc<Freshness>) -> Repo {
        self.freshness = Some(freshness);
        self
    }

    /// The gate set by `with_freshness`, if any.
    pub fn freshness(&self) -> Option<&Arc<Freshness>> {
        self.freshness.as_ref()
    }

    /// The git repos at or below the root: the root itself, `stone/`, and the code and doc roots.
    /// Looked up on every call (a few `stat`s), so a long-running server sees a repo that is
    /// cloned or removed after it started; callers keep the value for the whole operation.
    pub fn repos(&self) -> Repos {
        let config = &self.store.config;
        let dirs: Vec<String> = config.code_roots.iter().chain(&config.doc_roots).cloned().collect();
        Repos::discover(&self.store.layout.root, &dirs)
    }

    /// The same repo, with writes stamped by `actor` instead of `STONE_AGENT` or the git user.
    pub fn with_author(mut self, actor: String) -> Repo {
        self.author = Some(actor);
        self
    }

    /// The actor set by `with_author`, if any.
    pub(crate) fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    /// The derived index at `.stone/index.db`, rebuilt when missing or another schema.
    pub fn index(&self) -> Result<Index, EngineError> {
        let path = self.store.layout.index_path();
        Index::open(&path).map_err(|err| match err {
            IndexError::Io { .. } | IndexError::Sqlite(_) if !can_write(&path) => {
                EngineError::Invalid("cannot write .stone/index.db; check permissions".into())
            }
            other => other.into(),
        })
    }

    pub fn snapshot(&self) -> Result<Snapshot, EngineError> {
        Ok(Snapshot {
            entries: self.store.scan()?,
            scheme: self.store.config.scheme(),
        })
    }
}
