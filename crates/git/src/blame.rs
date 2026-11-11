//! `git blame --porcelain` output to lines.

use std::collections::HashMap;

use crate::{Git, GitError};

/// Where one line of a file came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub sha: String,
    /// The line number in the commit that introduced it.
    pub orig_line: u32,
    /// The line number in the blamed file.
    pub final_line: u32,
    /// The file name in that commit, which differs from the blamed one after a rename.
    pub path: String,
}

impl BlameLine {
    /// The line is not in any commit yet (an edit in the working tree).
    pub fn is_uncommitted(&self) -> bool {
        self.sha.bytes().all(|b| b == b'0')
    }
}

/// A header is `<sha> <orig> <final> [<group size>]`.
fn parse_header(line: &str) -> Option<(String, u32, u32)> {
    let mut parts = line.split(' ');
    let sha = parts.next()?;
    if sha.len() < 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let orig = parts.next()?.parse().ok()?;
    let line_no = parts.next()?.parse().ok()?;
    Some((sha.to_string(), orig, line_no))
}

pub fn parse_blame(text: &str) -> Vec<BlameLine> {
    let mut out = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut current: Option<(String, u32, u32)> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with('\t') {
            if let Some((sha, orig_line, final_line)) = current.take() {
                let path = names.get(&sha).cloned().unwrap_or_default();
                out.push(BlameLine {
                    sha,
                    orig_line,
                    final_line,
                    path,
                });
            }
        } else if let Some(header) = parse_header(line) {
            current = Some(header);
        } else if let Some(name) = line.strip_prefix("filename ")
            && let Some((sha, _, _)) = &current
        {
            names.insert(sha.clone(), name.to_string());
        }
    }
    out
}

impl Git {
    /// Blame of the working copy of `path`: all lines, or the 1-based inclusive
    /// range `lines`. Uncommitted lines carry the all-zero sha.
    pub fn blame(&self, path: &str, lines: Option<(u32, u32)>) -> Result<Vec<BlameLine>, GitError> {
        let range = lines.map(|(from, to)| format!("-L{from},{to}"));
        let mut args = vec!["blame", "--porcelain", "-w", "-M"];
        if let Some(range) = &range {
            args.push(range);
        }
        args.push("--");
        args.push(path);
        Ok(parse_blame(&self.run(&args)?))
    }
}
