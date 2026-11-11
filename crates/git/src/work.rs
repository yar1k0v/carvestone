//! Changes in the working tree that are not committed yet.

use crate::{FileChange, Git, GitError};

/// `git diff --name-status -z` output: `M\0path\0`, `R100\0old\0new\0`.
pub fn parse_name_status_z(text: &str) -> Vec<FileChange> {
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    let mut out = Vec::new();
    while let Some(status) = tokens.next() {
        let Some(letter) = status.chars().next() else {
            continue;
        };
        let Some(first) = tokens.next() else {
            break;
        };
        if matches!(letter, 'R' | 'C') {
            let Some(new) = tokens.next() else {
                break;
            };
            out.push(FileChange {
                status: letter,
                path: new.to_string(),
                old_path: Some(first.to_string()),
            });
        } else {
            out.push(FileChange {
                status: letter,
                path: first.to_string(),
                old_path: None,
            });
        }
    }
    out
}

impl Git {
    /// Tracked changes against HEAD (staged or not) plus untracked files that
    /// are not ignored, limited to the pathspecs in `paths` when given. Sorted by path.
    pub fn working_changes(&self, paths: &[&str]) -> Result<Vec<FileChange>, GitError> {
        let tail: Vec<&str> = if paths.is_empty() {
            Vec::new()
        } else {
            std::iter::once("--").chain(paths.iter().copied()).collect()
        };

        let mut diff = vec!["diff", "--name-status", "-z", "-M", "--relative", "--no-ext-diff"];
        // Without a first commit there is no HEAD to compare to; the index is all there is.
        diff.push(if self.head()?.is_some() { "HEAD" } else { "--cached" });
        diff.extend(&tail);
        let mut changes = parse_name_status_z(&self.run(&diff)?);

        let mut others = vec!["ls-files", "--others", "--exclude-standard", "-z"];
        others.extend(&tail);
        for path in self.run(&others)?.split('\0').filter(|p| !p.is_empty()) {
            changes.push(FileChange {
                status: '?',
                path: path.to_string(),
                old_path: None,
            });
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        changes.dedup_by(|a, b| a.path == b.path);
        Ok(changes)
    }

    /// Every file of the working tree under this folder, relative to it: the tracked ones, plus
    /// untracked files that are not ignored when `untracked` is set. A nested repo that is not a
    /// submodule shows as `dir/`. In no particular order.
    pub fn ls_files(&self, untracked: bool) -> Result<Vec<String>, GitError> {
        let mut args = vec!["ls-files", "-z", "--cached"];
        if untracked {
            args.extend(["--others", "--exclude-standard"]);
        }
        Ok(self
            .run(&args)?
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Whether the index holds `path` (committed, or added and not yet committed).
    pub fn is_tracked(&self, path: &str) -> Result<bool, GitError> {
        let out = self.exec(&["ls-files", "--error-unmatch", "--", path], None)?;
        Ok(out.code == 0)
    }

    /// Whether the working copy of a tracked `path` differs from HEAD, staged or not.
    pub fn differs_from_head(&self, path: &str) -> Result<bool, GitError> {
        let args = ["diff", "--quiet", "--no-ext-diff", "HEAD", "--", path];
        let out = self.exec(&args, None)?;
        match out.code {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(crate::command_error(&args, &out)),
        }
    }
}
