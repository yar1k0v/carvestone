//! One read-edit-write flow for the files the user owns in the repository: `.claude/settings.json`
//! and `CLAUDE.md` or `AGENTS.md`.

use carvestone_store::write::write_atomic;

use crate::repo::{EngineError, Repo};

impl Repo {
    /// Runs `edit` on the text of `path` under the store lock and writes the result when it differs.
    /// `label` names the file in errors. Returns whether the file changed.
    ///
    /// One flow for every file the user owns, so the lock, the re-read and the minimal write rule
    /// hold the same way for each of them.
    pub(crate) fn rewrite_root_file(
        &self,
        path: &std::path::Path,
        label: &str,
        edit: impl FnOnce(&str) -> Result<String, EngineError>,
    ) -> Result<bool, EngineError> {
        let lock = self.store.lock()?;
        let text = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8(bytes)
                .map_err(|_| EngineError::Invalid(format!("{label} is not valid UTF-8; nothing was written")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(EngineError::Invalid(format!("cannot read {label}: {e}"))),
        };
        let next = edit(&text)?;
        let changed = next != text;
        if changed {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder)
                    .map_err(|e| EngineError::Invalid(format!("cannot create {}: {e}", folder.display())))?;
            }
            write_atomic(path, &next)?;
        }
        // Held to here so two writers never replace the file from the same stale read.
        drop(lock);
        Ok(changed)
    }
}
