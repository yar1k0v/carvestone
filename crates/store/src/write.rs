//! Atomic file replace and create.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use carvestone_core::ItemId;

use crate::lock::WriteLock;
use crate::store::{Store, StoreError};

const RENAME_TRIES: u32 = 8;
const RENAME_PAUSE: Duration = Duration::from_millis(15);

fn io_err(path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Hidden and without a `.md` suffix, so a leftover from a crash is never scanned as an item.
fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.tmp-{}", std::process::id()))
}

/// Windows reports a target that an editor or scanner holds open as access denied (5),
/// sharing violation (32) or lock violation (33). Elsewhere such errors are real.
fn is_busy(e: &std::io::Error) -> bool {
    cfg!(windows) && (e.kind() == ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33)))
}

fn rename_retry(from: &Path, to: &Path) -> Result<(), StoreError> {
    for attempt in 0..RENAME_TRIES {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if is_busy(&e) => {
                // A directory or read-only target will not free up; retrying only hides the cause.
                if fs::metadata(to).is_ok_and(|m| m.is_dir() || m.permissions().readonly()) {
                    return Err(io_err(to, e));
                }
                if attempt + 1 < RENAME_TRIES {
                    std::thread::sleep(RENAME_PAUSE * (attempt + 1));
                }
            }
            Err(e) => return Err(io_err(to, e)),
        }
    }
    Err(StoreError::Busy(to.display().to_string()))
}

fn write_temp(temp: &Path, text: &str) -> Result<(), StoreError> {
    let mut file = File::create(temp).map_err(|e| io_err(temp, e))?;
    file.write_all(text.as_bytes()).map_err(|e| io_err(temp, e))?;
    // On disk before the rename, so a crash never leaves a renamed empty file.
    file.sync_all().map_err(|e| io_err(temp, e))
}

/// Writes `text` next to `path` and renames it over `path`. The temp file is removed
/// when anything fails; the text is written byte for byte.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    }
    let temp = temp_path(path);
    let result = write_temp(&temp, text).and_then(|()| rename_retry(&temp, path));
    if result.is_err() {
        // The first error is the one worth reporting.
        let _ = fs::remove_file(&temp);
    }
    result
}

/// True when the folder of `path` holds a file with the same name ignoring ASCII case:
/// `cs-7.md` and `CS-7.md` are one file on NTFS and two confusable ones elsewhere.
fn taken(path: &Path) -> Result<bool, StoreError> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(false);
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_err(dir, e)),
    };
    let want = name.to_string_lossy();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case(&want) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True when the folder of `dst` holds an entry named exactly like `dst` that is not `src`.
/// On a case-sensitive filesystem this is a different file a rename would overwrite.
fn clobbers(dst: &Path, src: &Path) -> Result<bool, StoreError> {
    let (Some(dir), Some(name)) = (dst.parent(), dst.file_name()) else {
        return Ok(false);
    };
    if src.file_name() == Some(name) {
        return Ok(false);
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_err(dir, e)),
    };
    for entry in entries {
        if entry.map_err(|e| io_err(dir, e))?.file_name() == name {
            return Ok(true);
        }
    }
    Ok(false)
}

impl Store {
    /// Replaces the item's file, or creates it. The lock must cover the read that
    /// produced `text`, so two writers cannot overwrite each other from a stale copy.
    pub fn save(&self, lock: &WriteLock, id: &ItemId, text: &str) -> Result<(), StoreError> {
        lock.check(&self.layout)?;
        write_atomic(&self.layout.item_path(id), text)
    }

    /// Creates the item's file; `Exists` when the name is taken in any letter case.
    pub fn create(&self, lock: &WriteLock, id: &ItemId, text: &str) -> Result<(), StoreError> {
        lock.check(&self.layout)?;
        let path = self.layout.item_path(id);
        if taken(&path)? {
            return Err(StoreError::Exists(id.to_string()));
        }
        write_atomic(&path, text)
    }

    /// Deletes the item's file. The lock must cover the read that decided the file may go.
    pub fn remove(&self, lock: &WriteLock, id: &ItemId) -> Result<(), StoreError> {
        lock.check(&self.layout)?;
        let path = self.layout.item_path(id);
        for attempt in 0..RENAME_TRIES {
            match fs::remove_file(&path) {
                Ok(()) => return Ok(()),
                Err(e) if is_busy(&e) => {
                    if attempt + 1 < RENAME_TRIES {
                        std::thread::sleep(RENAME_PAUSE * (attempt + 1));
                    }
                }
                Err(e) => return Err(io_err(&path, e)),
            }
        }
        Err(StoreError::Busy(path.display().to_string()))
    }

    /// Moves an item's file to a new id; `Exists` when the target name is taken,
    /// except for a change of letter case alone.
    pub fn rename(&self, lock: &WriteLock, from: &ItemId, to: &ItemId) -> Result<(), StoreError> {
        lock.check(&self.layout)?;
        let src = self.layout.item_path(from);
        let dst = self.layout.item_path(to);
        fs::metadata(&src).map_err(|e| io_err(&src, e))?;
        let case_only = src.to_string_lossy().eq_ignore_ascii_case(&dst.to_string_lossy());
        let blocked = if case_only { clobbers(&dst, &src)? } else { taken(&dst)? };
        if blocked {
            return Err(StoreError::Exists(to.to_string()));
        }
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
        }
        rename_retry(&src, &dst)
    }
}
