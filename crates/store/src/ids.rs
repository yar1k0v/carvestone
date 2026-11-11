//! Next free number for numbered items.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use carvestone_core::{IdScheme, ItemId, NumberedKind};

use crate::lock::WriteLock;
use crate::store::{Store, StoreError};

/// The number to use for a new item and, when branches could not all be checked, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub number: u32,
    pub warning: Option<String>,
}

impl Store {
    /// Highest number of `kind` in the working tree and in every local branch and
    /// remote-tracking ref, plus one. Call it with the lock held and create the item
    /// before the lock is dropped, or two writers can pick the same number.
    pub fn next_number(&self, lock: &WriteLock, kind: NumberedKind) -> Result<Allocation, StoreError> {
        lock.check(&self.layout)?;
        let scheme = self.config.scheme();
        let folder = scheme
            .numbered_id(kind, 1)
            .as_str()
            .split_once('/')
            .map(|(f, _)| f.to_string())
            .unwrap_or_default();
        let in_tree = tree_max(&self.layout.data_dir.join(&folder), &scheme, kind, &folder)?;
        // Git runs inside `stone/`, so the repo that holds the items answers: the root repo, or
        // `stone/` as a repo of its own in a product root that is not one.
        let (in_branches, warning) = branch_max(&self.layout.data_dir, &scheme, kind, &folder);
        Ok(Allocation {
            number: in_tree.max(in_branches).saturating_add(1),
            warning,
        })
    }
}

/// The number in a file name such as `CS-007.md`, when it is an item of `kind`.
fn number_of(scheme: &IdScheme, kind: NumberedKind, folder: &str, file: &str) -> Option<u32> {
    let stem = file.strip_suffix(".md")?;
    match scheme.number(&ItemId::from_path(&format!("{folder}/{stem}"))) {
        Some((k, n)) if k == kind => Some(n),
        _ => None,
    }
}

fn tree_max(dir: &Path, scheme: &IdScheme, kind: NumberedKind, folder: &str) -> Result<u32, StoreError> {
    let io = |source: std::io::Error| StoreError::Io {
        path: dir.display().to_string(),
        source,
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(io(e)),
    };
    let mut max = 0;
    for entry in entries {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(n) = number_of(scheme, kind, folder, &name) {
            max = max.max(n);
        }
    }
    Ok(max)
}

/// Output of a git command run in `root`; `None` when git is missing or the command fails.
fn git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git").current_dir(root).args(args).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn unchecked(why: &str) -> (u32, Option<String>) {
    (
        0,
        Some(format!(
            "{why}; branches were not checked, the number comes from the working tree only"
        )),
    )
}

/// Highest number of `kind` over all local and remote-tracking refs, and a warning when
/// some or all of them could not be read. `root` is the folder that holds the items (`stone/`).
fn branch_max(root: &Path, scheme: &IdScheme, kind: NumberedKind, folder: &str) -> (u32, Option<String>) {
    let Some(prefix) = git(root, &["rev-parse", "--show-prefix"]) else {
        return unchecked("not a git repository, or git is missing");
    };
    let prefix = String::from_utf8_lossy(&prefix)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    let Some(refs) = git(
        root,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    ) else {
        return unchecked("could not list the git refs");
    };
    // `--full-name` prints paths from the git top level, so keep only our own folder. The
    // prefix is where `root` sits in the repo; the pathspec is relative to `root`.
    let wanted = format!("{prefix}{folder}/");
    let pathspec = format!("{folder}/");
    let mut seen = HashSet::new();
    let (mut max, mut total, mut failed) = (0, 0u32, 0u32);
    for line in String::from_utf8_lossy(&refs).lines() {
        let Some((oid, name)) = line.split_once(' ') else {
            continue;
        };
        // refs/remotes/<remote>/HEAD points at a branch that is listed on its own.
        if name.starts_with("refs/remotes/") && name.ends_with("/HEAD") {
            continue;
        }
        if !seen.insert(oid.to_string()) {
            continue;
        }
        total += 1;
        let listed = git(
            root,
            &["ls-tree", "--name-only", "-z", "--full-name", oid, "--", &pathspec],
        );
        let Some(listed) = listed else {
            failed += 1;
            continue;
        };
        for path in listed.split(|b| *b == 0) {
            let path = String::from_utf8_lossy(path);
            if let Some(file) = path.strip_prefix(&wanted)
                && let Some(n) = number_of(scheme, kind, folder, file)
            {
                max = max.max(n);
            }
        }
    }
    let warning = (failed > 0)
        .then(|| format!("could not read {failed} of {total} git refs; numbers on those branches were not checked"));
    (max, warning)
}
