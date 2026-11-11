//! Single-object reads: a commit's diff of one file, a file's blob, the hooks folder.

use std::path::PathBuf;

use crate::{Git, GitError};

impl Git {
    /// The patch `sha` made to `path`, empty when it did not touch it.
    pub fn show_file_diff(&self, sha: &str, path: &str) -> Result<String, GitError> {
        self.show_diff(sha, &[path])
    }

    /// The patch `sha` made to `paths`; name a file's old and new path to see a rename.
    pub fn show_diff(&self, sha: &str, paths: &[&str]) -> Result<String, GitError> {
        let mut args = vec!["show", "--format=", "--no-color", "--no-ext-diff", "-M", sha, "--"];
        args.extend(paths);
        self.run(&args)
    }

    /// The text of `path` as `sha` had it; `None` when that commit has no such file.
    pub fn file_at(&self, sha: &str, path: &str) -> Result<Option<String>, GitError> {
        let spec = format!("{sha}:./{path}");
        let out = self.exec(&["show", &spec], None)?;
        Ok((out.code == 0).then_some(out.stdout))
    }

    /// The blob hash of `path` in HEAD, `None` when HEAD does not hold the file.
    pub fn head_blob(&self, path: &str) -> Result<Option<String>, GitError> {
        let spec = format!("HEAD:./{path}");
        let out = self.exec(&["rev-parse", "--verify", "--quiet", &spec], None)?;
        if out.code == 0 {
            return Ok(Some(out.stdout.trim().to_string()));
        }
        // Not in HEAD, or no HEAD at all; a missing repo already failed above.
        self.run(&["rev-parse", "--git-dir"])?;
        Ok(None)
    }

    /// Where hooks live: `core.hooksPath` when set, else the repository's `hooks` folder.
    pub fn hooks_dir(&self) -> Result<PathBuf, GitError> {
        let default = self.run(&["rev-parse", "--git-path", "hooks"])?;
        let configured = self.exec(&["config", "--get", "core.hooksPath"], None)?;
        let path = configured.stdout.trim();
        if configured.code == 0 && !path.is_empty() {
            return self.resolve_hooks_path(path);
        }
        Ok(self.dir.join(default.trim()))
    }

    fn resolve_hooks_path(&self, configured: &str) -> Result<PathBuf, GitError> {
        if let Some(rest) = configured.strip_prefix("~/")
            && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        {
            return Ok(PathBuf::from(home).join(rest));
        }
        // A relative hooks path is relative to the top of the working tree.
        let top = self.run(&["rev-parse", "--show-toplevel"])?;
        Ok(PathBuf::from(top.trim()).join(configured))
    }
}
