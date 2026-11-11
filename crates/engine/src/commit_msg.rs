//! `stone commit-msg`: the trailers for the task in progress, the opt-in hook
//! that checks a message carries a `Refs:` trailer, and its install and removal.
//!
//! The hook file is the only thing in this module that writes anything.

use std::fs;
use std::path::PathBuf;

use carvestone_core::ItemType;

use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;

/// First line the hook carries after its shebang; the file is stone's only when it has it.
pub const HOOK_MARKER: &str = "# stone commit-msg hook";

const HOOK: &str = "#!/bin/sh\n\
# stone commit-msg hook\n\
# Installed by `stone commit-msg --install-hook`; remove it with `--uninstall-hook`.\n\
command -v stone >/dev/null 2>&1 || exit 0\n\
exec stone commit-msg --check \"$1\"\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    Installed,
    AlreadyInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallState {
    Removed,
    NotInstalled,
}

/// The trailer lines for `task`, or for the only task in progress when none is named.
pub fn trailers_for(repo: &Repo, task: Option<&str>) -> Result<Vec<String>, EngineError> {
    let snapshot = repo.snapshot()?;
    let entry = match task {
        Some(input) => {
            let (entry, _) = snapshot.resolve(input)?;
            if entry.item.as_ref().and_then(|i| i.item_type()) != Some(ItemType::Task) {
                return Err(EngineError::NotATask {
                    id: snapshot.label(entry),
                });
            }
            entry
        }
        None => {
            let active: Vec<_> = snapshot
                .entries
                .iter()
                .filter(|e| {
                    e.item.as_ref().is_some_and(|i| {
                        i.item_type() == Some(ItemType::Task) && i.fields.status.as_deref() == Some("in-progress")
                    })
                })
                .collect();
            match active.as_slice() {
                [] => {
                    return Err(EngineError::Invalid(
                        "no task is in progress; pass --task CS-n".to_string(),
                    ));
                }
                [one] => *one,
                many => {
                    let mut labels: Vec<String> = many.iter().map(|e| snapshot.label(e)).collect();
                    labels.sort();
                    return Err(EngineError::Invalid(format!(
                        "{} tasks are in progress ({}); pass --task",
                        many.len(),
                        labels.join(", ")
                    )));
                }
            }
        }
    };
    let Some(item) = &entry.item else {
        return Err(EngineError::NotATask {
            id: snapshot.label(entry),
        });
    };

    let mut lines = vec![format!("Refs: {}", snapshot.label(entry))];
    if let Some(plan) = item.fields.plan.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        lines.push(format!("Plan: {plan}"));
    }
    for link in &item.fields.links {
        if let Some(label) = decision_label(&snapshot, link)
            && !lines.contains(&format!("Decision: {label}"))
        {
            lines.push(format!("Decision: {label}"));
        }
    }
    Ok(lines)
}

/// The label of the decision `link` names; `None` for any other target.
fn decision_label(snapshot: &Snapshot, link: &str) -> Option<String> {
    let (entry, _) = snapshot.resolve(link).ok()?;
    let is_decision = entry.item.as_ref().and_then(|i| i.item_type()) == Some(ItemType::Decision);
    is_decision.then(|| snapshot.label(entry))
}

/// One repo's hook: the repo folder (relative to the root, `""` for the root) and its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookFile {
    pub repo: String,
    pub path: PathBuf,
}

/// The hook state `install_hooks` left in one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInstall {
    pub hook: HookFile,
    pub state: HookState,
}

/// The hook state `uninstall_hooks` left in one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRemoval {
    pub hook: HookFile,
    pub state: UninstallState,
}

/// Where the hook file lives in every git repo of the product: the hooks folder git uses,
/// `commit-msg` inside it. Without any repo that is git's "not available" error.
pub fn hook_files(repo: &Repo) -> Result<Vec<HookFile>, EngineError> {
    let repos = repo.repos();
    if repos.all().is_empty() {
        return Err(carvestone_git::GitError::Unavailable.into());
    }
    let mut files: Vec<HookFile> = Vec::new();
    for r in repos.all() {
        let path = r.git().hooks_dir()?.join("commit-msg");
        // Repos that share one hooks folder (`core.hooksPath`) share the file.
        if !files.iter().any(|f| f.path == path) {
            files.push(HookFile {
                repo: r.dir().to_string(),
                path,
            });
        }
    }
    Ok(files)
}

/// The hook file of the first repo found (the root repo when the root is one).
pub fn hook_path(repo: &Repo) -> Result<PathBuf, EngineError> {
    Ok(hook_files(repo)?.remove(0).path)
}

fn is_ours(text: &str) -> bool {
    text.lines().any(|l| l.trim_end() == HOOK_MARKER)
}

/// Installs the hook in every repo found, and says what each one did. A foreign hook in any
/// repo stops everything before a file is written.
pub fn install_hooks(repo: &Repo) -> Result<Vec<HookInstall>, EngineError> {
    let files = hook_files(repo)?;
    let several = files.len() > 1;
    let mut pending = Vec::new();
    for hook in files {
        match fs::read(&hook.path) {
            Ok(bytes) if is_ours(&String::from_utf8_lossy(&bytes)) => pending.push((hook, HookState::AlreadyInstalled)),
            Ok(_) => {
                let place = if several {
                    format!(" (in {})", repo_label(&hook.repo))
                } else {
                    String::new()
                };
                return Err(EngineError::Invalid(format!(
                    "a different commit-msg hook exists{place}; remove it or add the check by hand"
                )));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => pending.push((hook, HookState::Installed)),
            Err(err) => return Err(io_error(&hook.path, &err)),
        }
    }
    let mut done = Vec::new();
    for (hook, state) in pending {
        if state == HookState::Installed {
            if let Some(dir) = hook.path.parent() {
                fs::create_dir_all(dir).map_err(|err| io_error(dir, &err))?;
            }
            fs::write(&hook.path, HOOK).map_err(|err| io_error(&hook.path, &err))?;
            make_executable(&hook.path)?;
        }
        done.push(HookInstall { hook, state });
    }
    Ok(done)
}

/// Removes stone's hook from every repo found. A hook that is not stone's stops everything
/// before a file is removed.
pub fn uninstall_hooks(repo: &Repo) -> Result<Vec<HookRemoval>, EngineError> {
    let files = hook_files(repo)?;
    let several = files.len() > 1;
    let mut pending = Vec::new();
    for hook in files {
        match fs::read(&hook.path) {
            Ok(bytes) if is_ours(&String::from_utf8_lossy(&bytes)) => pending.push((hook, UninstallState::Removed)),
            Ok(_) => {
                let place = if several {
                    format!(" in {}", repo_label(&hook.repo))
                } else {
                    String::new()
                };
                return Err(EngineError::Invalid(format!(
                    "the commit-msg hook{place} is not stone's; remove it by hand"
                )));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                pending.push((hook, UninstallState::NotInstalled))
            }
            Err(err) => return Err(io_error(&hook.path, &err)),
        }
    }
    let mut done = Vec::new();
    for (hook, state) in pending {
        if state == UninstallState::Removed {
            fs::remove_file(&hook.path).map_err(|err| io_error(&hook.path, &err))?;
        }
        done.push(HookRemoval { hook, state });
    }
    Ok(done)
}

/// `Installed` when any repo got the hook now, else `AlreadyInstalled`. For a one-repo root
/// that is the one hook; `install_hooks` has the per-repo answer.
pub fn install_hook(repo: &Repo) -> Result<HookState, EngineError> {
    let done = install_hooks(repo)?;
    let fresh = done.iter().any(|d| d.state == HookState::Installed);
    Ok(if fresh {
        HookState::Installed
    } else {
        HookState::AlreadyInstalled
    })
}

/// `Removed` when any repo lost the hook, else `NotInstalled`; `uninstall_hooks` has the
/// per-repo answer.
pub fn uninstall_hook(repo: &Repo) -> Result<UninstallState, EngineError> {
    let done = uninstall_hooks(repo)?;
    let gone = done.iter().any(|d| d.state == UninstallState::Removed);
    Ok(if gone {
        UninstallState::Removed
    } else {
        UninstallState::NotInstalled
    })
}

/// A repo folder as a person reads it: `.` for the product root.
pub fn repo_label(dir: &str) -> &str {
    if dir.is_empty() { "." } else { dir }
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<(), EngineError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|err| io_error(path, &err))
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<(), EngineError> {
    Ok(())
}

fn io_error(path: &std::path::Path, err: &std::io::Error) -> EngineError {
    EngineError::Invalid(format!("cannot write {}: {err}", path.display()))
}

/// Fails unless the commit message has a `Refs:` trailer. Merge, revert, fixup
/// and squash messages pass; `#` lines are git's comments and are ignored.
pub fn check_message(text: &str) -> Result<(), EngineError> {
    let mut first = true;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.starts_with("# ------------------------ >8") {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        if first && !line.trim().is_empty() {
            first = false;
            if ["Merge ", "Revert ", "fixup!", "squash!"]
                .iter()
                .any(|p| line.starts_with(p))
            {
                return Ok(());
            }
        }
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("refs")
            && !value.trim().is_empty()
        {
            return Ok(());
        }
    }
    Err(EngineError::Invalid(
        "no Refs trailer; add one with `stone commit-msg`".to_string(),
    ))
}
