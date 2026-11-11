//! Build-then-rename rebuild: `.stone/index.db.new` replaces `index.db` while
//! `.stone/index.lock` is held, so a reader keeps the file it already has.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::{Index, IndexError, schema};

/// How long another process may hold the rebuild lock.
const LOCK_WAIT: Duration = Duration::from_secs(5);
/// How long Windows may report the rename target as busy.
const RENAME_WAIT: Duration = Duration::from_secs(5);
const RETRY_STEP: Duration = Duration::from_millis(50);

static REBUILDS: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Rebuilds this process ran for `path`. Tests use it to check that four
/// concurrent opens rebuild once.
#[doc(hidden)]
pub fn rebuild_count(path: &Path) -> usize {
    REBUILDS
        .lock()
        .map_or(0, |counts| counts.get(path).copied().unwrap_or(0))
}

pub(crate) fn rebuild(path: &Path, version: i64) -> Result<Index, IndexError> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent).map_err(|source| IndexError::Io {
        path: parent.display().to_string(),
        source,
    })?;

    let lock = Lock::take(path)?;
    // Another process may have rebuilt while we waited for the lock.
    if let Some(index) = Index::current(path, version)? {
        return Ok(index);
    }

    let fresh = path.with_extension("db.new");
    remove_quietly(&fresh);
    if let Err(err) = build(&fresh, version).and_then(|()| swap(path, &fresh)) {
        remove_quietly(&fresh);
        return Err(err);
    }

    let conn = Connection::open(path)?;
    crate::configure(&conn)?;
    drop(lock);
    record_rebuild(path);
    Ok(Index { conn })
}

/// The exclusive lock on `.stone/index.lock`. SQLite releases it when the
/// process exits, so a crash cannot leave a stale lock behind.
struct Lock {
    _conn: Connection,
}

impl Lock {
    fn take(index: &Path) -> Result<Lock, IndexError> {
        let conn = Connection::open(index.with_extension("lock"))?;
        conn.busy_timeout(LOCK_WAIT)?;
        conn.execute_batch("BEGIN EXCLUSIVE")
            .map_err(|_| IndexError::RebuildBusy {
                path: index.display().to_string(),
            })?;
        Ok(Lock { _conn: conn })
    }
}

fn build(path: &Path, version: i64) -> Result<(), IndexError> {
    let conn = Connection::open(path)?;
    // One transaction: every statement on its own would sync the file to disk again, which made
    // a first open cost about half a second on Windows.
    let tx = conn.unchecked_transaction()?;
    schema::create(&tx, version)?;
    tx.commit()?;
    // Closed before the rename: a journal or WAL sidecar would not follow the file.
    drop(conn);
    Ok(())
}

fn swap(path: &Path, fresh: &Path) -> Result<(), IndexError> {
    if is_garbage(path) {
        // Keep a foreign file for inspection instead of dropping it.
        let _ = fs::rename(path, path.with_extension("db.bad"));
    }
    if replace(fresh, path).is_err() {
        return Err(IndexError::RebuildBusy {
            path: path.display().to_string(),
        });
    }
    // The swapped-out file's sidecars belong to a file that is no longer there.
    for suffix in ["-wal", "-shm"] {
        remove_quietly(&sidecar(path, suffix));
    }
    Ok(())
}

fn is_garbage(path: &Path) -> bool {
    const HEADER: &[u8] = b"SQLite format 3\0";
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 16];
    let Ok(len) = file.read(&mut head) else {
        return false;
    };
    len > 0 && &head[..len] != HEADER
}

fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let started = Instant::now();
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) if busy(&err) && started.elapsed() < RENAME_WAIT => thread::sleep(RETRY_STEP),
            Err(err) => return Err(err),
        }
    }
}

#[cfg(windows)]
fn busy(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn busy(_: &std::io::Error) -> bool {
    false
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_quietly(path: &Path) {
    let _ = fs::remove_file(path);
}

fn record_rebuild(path: &Path) {
    if let Ok(mut counts) = REBUILDS.lock() {
        *counts.entry(path.to_path_buf()).or_default() += 1;
    }
}
