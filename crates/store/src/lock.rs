//! Exclusive write lock on `.stone/lock`.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::layout::Layout;
use crate::store::{Store, StoreError};

/// How long a writer waits for another stone process before giving up.
const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

/// Proof that `.stone/lock` is held. Every store write takes one, so none can run unlocked.
#[derive(Debug)]
pub struct WriteLock {
    file: File,
    path: PathBuf,
}

impl WriteLock {
    /// True when this lock belongs to the store with `layout`.
    pub(crate) fn guards(&self, layout: &Layout) -> bool {
        self.path == layout.lock_path()
    }

    /// `guards` as an error, so a lock from another store is refused in release builds too.
    pub(crate) fn check(&self, layout: &Layout) -> Result<(), StoreError> {
        if self.guards(layout) {
            return Ok(());
        }
        Err(StoreError::Io {
            path: self.path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "write lock belongs to another store"),
        })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        // Closing the handle releases the lock anyway; an error here changes nothing.
        let _ = self.file.unlock();
    }
}

impl Store {
    /// Takes the exclusive write lock for a whole read-modify-write. The OS drops it
    /// when the process dies, so there is no stale lock file to clean up.
    pub fn lock(&self) -> Result<WriteLock, StoreError> {
        let path = self.layout.lock_path();
        let io = |source: std::io::Error| StoreError::Io {
            path: path.display().to_string(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(io)?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(WriteLock { file, path }),
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= WAIT {
                        return Err(StoreError::Busy(".stone/lock".into()));
                    }
                    std::thread::sleep(POLL);
                }
                Err(TryLockError::Error(e)) => return Err(io(e)),
            }
        }
    }
}
