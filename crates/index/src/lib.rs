//! Carvestone's derived index. Never authoritative: it is rebuilt from files.
//!
//! A schema version or table-shape mismatch rebuilds the file instead of
//! migrating it. The rebuild writes `.stone/index.db.new` and renames it over
//! `index.db` while holding an exclusive lock on `.stone/index.lock`, so
//! readers keep the old file until they reopen.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, Error as SqliteError, ErrorCode};
use thiserror::Error;

mod rebuild;
mod schema;
mod schema_git;

pub use rebuild::rebuild_count;

/// The schema this build writes. Bump it when the tables change, or when the rows a file
/// yields change (7: diagram links, 8: parse warnings, 9: setting and table links, 10: topic keys,
/// setting values and Dart navigation helpers, 11: no framework links); the next open rebuilds
/// the file.
pub const SCHEMA_VERSION: i64 = 11;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("cannot write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot rebuild {path}; close other stone processes")]
    RebuildBusy { path: String },
}

pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open `path`, rebuilding it when it is missing, damaged or another schema.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        Self::open_versioned(path, SCHEMA_VERSION)
    }

    /// Open expecting `version`. Tests use it to stand up an older schema; a
    /// mismatch is rebuilt, never migrated.
    pub fn open_versioned(path: &Path, version: i64) -> Result<Self, IndexError> {
        match Self::current(path, version)? {
            Some(index) => Ok(index),
            None => rebuild::rebuild(path, version),
        }
    }

    /// An empty index in memory, for tests and one-shot queries.
    pub fn in_memory() -> Result<Self, IndexError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        schema::create(&conn, SCHEMA_VERSION)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64, IndexError> {
        schema::version(&self.conn).ok_or(IndexError::Sqlite(SqliteError::QueryReturnedNoRows))
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// `None` when the file is missing, not a database or at another version.
    fn current(path: &Path, version: i64) -> Result<Option<Self>, IndexError> {
        if !path.exists() {
            return Ok(None);
        }
        let conn = Connection::open(path)?;
        if let Err(err) = configure(&conn) {
            return match err {
                SqliteError::SqliteFailure(failure, _) if is_damage(failure.code) => Ok(None),
                other => Err(other.into()),
            };
        }
        Ok(schema::matches(&conn, version).then_some(Self { conn }))
    }
}

fn is_damage(code: ErrorCode) -> bool {
    matches!(code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
}

fn configure(conn: &Connection) -> Result<(), SqliteError> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}
