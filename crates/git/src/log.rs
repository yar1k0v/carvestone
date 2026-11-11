//! `git log` output to commits.

use crate::{Git, GitError, parse_name_status_z};

/// The `--format=` body of every log call. Records start with 0x1e, fields
/// are split by 0x1f and the header ends with 0x1d; whatever follows up to
/// the next record is `--name-status -z` output.
pub const LOG_FORMAT: &str = "%x1e%H%x1f%P%x1f%an%x1f%aI%x1f%at%x1f%s%x1f%(trailers:only,unfold)%x1d";

/// One path touched by a commit (or by the working tree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// `A`, `M`, `D`, `R`, `C`, `T` or `?` for an untracked file.
    pub status: char,
    pub path: String,
    /// The path before a rename or copy.
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    /// Author date, strict ISO 8601 with the author's offset.
    pub date: String,
    /// Author date as Unix seconds.
    pub timestamp: i64,
    pub subject: String,
    /// `(key, value)` in message order; `Refs`, `Plan` and `Decision` lines
    /// with several comma-separated values give one pair per value.
    pub trailers: Vec<(String, String)>,
    pub files: Vec<FileChange>,
}

/// Trailer keys whose line may carry several comma-separated values.
const LIST_KEYS: [&str; 3] = ["refs", "plan", "decision"];

pub fn parse_log(text: &str) -> Vec<Commit> {
    let mut commits: Vec<Commit> = Vec::new();
    for commit in text.split('\x1e').filter_map(parse_record) {
        // With `-m` (git older than 2.31) a merge is printed once per parent
        // under the same header; the first block is the first parent's diff.
        if commits.last().is_some_and(|prev| prev.sha == commit.sha) {
            continue;
        }
        commits.push(commit);
    }
    commits
}

fn parse_record(record: &str) -> Option<Commit> {
    let (header, rest) = record.split_once('\x1d')?;
    let mut fields = header.splitn(7, '\x1f');
    let sha = fields.next()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    let parents = fields.next()?.split_whitespace().map(str::to_string).collect();
    let author = fields.next()?.trim_end_matches('\r').to_string();
    let date = fields.next()?.trim().to_string();
    let timestamp = fields.next()?.trim().parse().unwrap_or(0);
    let subject = fields.next()?.trim_end_matches('\r').to_string();
    let trailers = parse_trailers(fields.next().unwrap_or(""));
    // With `-z` the header is followed by a NUL and a newline, then NUL-separated entries.
    let files = parse_name_status_z(rest.trim_start_matches(['\0', '\n', '\r']));
    Some(Commit {
        sha,
        parents,
        author,
        date,
        timestamp,
        subject,
        trailers,
        files,
    })
}

fn parse_trailers(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.is_empty() {
            continue;
        }
        if LIST_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) {
            for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                out.push((key.to_string(), part.to_string()));
            }
        } else {
            out.push((key.to_string(), value.to_string()));
        }
    }
    out
}

/// Arguments shared by every log call, up to the revision and path selection.
/// `-z` keeps odd paths (quotes, tabs, newlines) unquoted; a merge lists the
/// files it changed against its first parent.
fn base_args() -> Result<Vec<String>, GitError> {
    let mut args = vec![
        "log".to_string(),
        format!("--format={LOG_FORMAT}"),
        "--name-status".to_string(),
        "-z".to_string(),
        "-M".to_string(),
        "--relative".to_string(),
        "--no-show-signature".to_string(),
    ];
    if crate::git_at_least(2, 31)? {
        args.push("--diff-merges=first-parent".to_string());
    } else {
        // Git 2.30 has no `--diff-merges`. `-m` limits neither the diff nor the
        // walk, but prints a merge once per parent; `parse_log` keeps the first
        // block, so the files are the first parent's and side branches stay in
        // the history.
        args.push("-m".to_string());
    }
    Ok(args)
}

fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// A line a commit added to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedLine {
    pub sha: String,
    /// Author date as Unix seconds.
    pub timestamp: i64,
    /// The file as that commit named it.
    pub path: String,
    /// The line without its `+`, and without a trailing CR.
    pub text: String,
}

/// Added lines of a `git log -p --format=%x1e%H%x1f%at` output that start with `prefix`.
pub fn parse_added_lines(text: &str, prefix: &str) -> Vec<AddedLine> {
    let mut out = Vec::new();
    for record in text.split('\x1e') {
        let Some((head, patch)) = record.split_once('\n') else {
            continue;
        };
        let mut fields = head.trim().splitn(2, '\x1f');
        let (Some(sha), Some(at)) = (fields.next(), fields.next()) else {
            continue;
        };
        let timestamp = at.trim().parse().unwrap_or(0);
        let mut path = String::new();
        let mut in_hunk = false;
        for line in patch.lines() {
            let line = line.trim_end_matches('\r');
            if line.starts_with("diff --git ") {
                in_hunk = false;
            } else if line.starts_with("@@ ") {
                in_hunk = true;
            } else if !in_hunk {
                // Header lines; `+++ /dev/null` marks a deleted file.
                if let Some(name) = line.strip_prefix("+++ ") {
                    path = name.strip_prefix("b/").unwrap_or("").trim_end_matches('\t').to_string();
                }
            } else if let Some(added) = line.strip_prefix('+')
                && added.starts_with(prefix)
            {
                out.push(AddedLine {
                    sha: sha.to_string(),
                    timestamp,
                    path: path.clone(),
                    text: added.to_string(),
                });
            }
        }
    }
    out
}

/// A branch without commits has an empty log, not an error.
fn empty_branch(err: &GitError) -> bool {
    matches!(err, GitError::Command { stderr, .. } if stderr.contains("does not have any commits yet"))
}

impl Git {
    /// Commits newest first. `range` is a revision or `a..b`; no range means HEAD.
    pub fn log(&self, range: Option<&str>, limit: Option<usize>, paths: &[&str]) -> Result<Vec<Commit>, GitError> {
        let mut args = base_args()?;
        if let Some(limit) = limit {
            args.push("-n".to_string());
            args.push(limit.to_string());
        }
        if let Some(range) = range {
            args.push(range.to_string());
        }
        self.log_with(args, paths)
    }

    /// The commits `shas` names, in that order.
    pub fn commits(&self, shas: &[String]) -> Result<Vec<Commit>, GitError> {
        if shas.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = base_args()?;
        args.push("--no-walk=unsorted".to_string());
        args.push("--stdin".to_string());
        let mut input = shas.join("\n");
        input.push('\n');
        let text = self.run_with_input(&as_strs(&args), &input)?;
        Ok(parse_log(&text))
    }

    /// Lines starting with `prefix` that commits added under `paths`, newest
    /// commit first. `pickaxe` is an extended regex that narrows the commits
    /// (`git log -G`), so the patches git prints stay small.
    pub fn added_lines(&self, pickaxe: &str, prefix: &str, paths: &[&str]) -> Result<Vec<AddedLine>, GitError> {
        let g = format!("-G{pickaxe}");
        let mut args = vec![
            "log",
            "--format=%x1e%H%x1f%at",
            "--no-color",
            "--no-ext-diff",
            // Fixed prefixes whatever `diff.noprefix` says, and paths relative to
            // the store folder, so the `+++` line can be read back.
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--relative",
            "-p",
            "-U0",
            &g,
            "--",
        ];
        args.extend(paths);
        match self.run(&args) {
            Ok(text) => Ok(parse_added_lines(&text, prefix)),
            Err(err) if empty_branch(&err) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }

    /// The sha and whole message of each commit whose message matches the extended regex `pattern`,
    /// newest first.
    pub fn messages_matching(&self, pattern: &str) -> Result<Vec<(String, String)>, GitError> {
        let grep = format!("--grep={pattern}");
        match self.run(&["log", "-E", &grep, "--format=%x1e%H%x1f%B"]) {
            Ok(text) => Ok(text
                .split('\x1e')
                .filter_map(|record| record.split_once('\x1f'))
                .map(|(sha, message)| (sha.trim().to_string(), message.to_string()))
                .collect()),
            Err(err) if empty_branch(&err) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }

    /// Commits that touched `path`, newest first, optionally across renames.
    pub fn file_log(&self, path: &str, follow: bool, limit: usize) -> Result<Vec<Commit>, GitError> {
        let mut args = base_args()?;
        if follow {
            args.push("--follow".to_string());
        }
        args.push("-n".to_string());
        args.push(limit.to_string());
        self.log_with(args, &[path])
    }

    fn log_with(&self, mut args: Vec<String>, paths: &[&str]) -> Result<Vec<Commit>, GitError> {
        if !paths.is_empty() {
            args.push("--".to_string());
            args.extend(paths.iter().map(|p| p.to_string()));
        }
        match self.run(&as_strs(&args)) {
            Ok(text) => Ok(parse_log(&text)),
            Err(err) if empty_branch(&err) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }
}
