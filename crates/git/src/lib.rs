//! Carvestone's view of git: the `git` CLI, nothing else.
//!
//! Every call runs `git --no-pager -C <dir> -c core.quotepath=off ...` with
//! `LC_ALL=C` and a 30 s timeout. Parsers take captured text, so they are
//! tested on canned samples; a few tests also run real git in temp repos.
//! This crate never touches SQLite and never writes to a repository.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

mod blame;
mod log;
mod show;
mod work;

pub use blame::{BlameLine, parse_blame};
pub use log::{AddedLine, Commit, FileChange, LOG_FORMAT, parse_added_lines, parse_log};
pub use work::parse_name_status_z;

/// Longest a single git call may run.
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("git is not available in this folder")]
    Unavailable,
    #[error("git 2.30 or newer is required; upgrade git")]
    TooOld,
    #[error("git {args} failed: {stderr}")]
    Command { args: String, stderr: String },
}

/// Captured result of one git call.
pub(crate) struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Whether a usable git is installed at all: `Unavailable` when it is missing, `TooOld` when
/// it is below the supported version. Says nothing about any folder.
pub fn check_installed() -> Result<(), GitError> {
    check_version()
}

/// A repository folder. Cheap to build; every method starts its own git process.
#[derive(Debug, Clone)]
pub struct Git {
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Git {
    pub fn new(dir: &Path) -> Git {
        Git {
            dir: dir.to_path_buf(),
            env: Vec::new(),
        }
    }

    /// The same folder with one more environment variable on every call.
    pub fn with_env(mut self, key: &str, value: &str) -> Git {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The commit HEAD points at; `None` on a branch without commits.
    pub fn head(&self) -> Result<Option<String>, GitError> {
        let out = self.exec(&["rev-parse", "--verify", "--quiet", "HEAD"], None)?;
        if out.code == 0 {
            return Ok(Some(out.stdout.trim().to_string()));
        }
        // No HEAD: an unborn branch is fine, anything else surfaces here.
        self.run(&["rev-parse", "--git-dir"])?;
        Ok(None)
    }

    /// Shas reachable per `range` (for example `HEAD` or `abc..HEAD`), parents before children.
    pub fn rev_list_reverse(&self, range: &str) -> Result<Vec<String>, GitError> {
        let out = self.run(&["rev-list", "--topo-order", "--reverse", range])?;
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Whether `ancestor` is reachable from `descendant` (a commit is its own ancestor).
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, GitError> {
        let out = self.exec(&["merge-base", "--is-ancestor", ancestor, descendant], None)?;
        match out.code {
            0 => Ok(true),
            1 => Ok(false),
            _ => Err(command_error(
                &["merge-base", "--is-ancestor", ancestor, descendant],
                &out,
            )),
        }
    }

    /// Whether the clone has cut history.
    pub fn is_shallow(&self) -> Result<bool, GitError> {
        Ok(self.run(&["rev-parse", "--is-shallow-repository"])?.trim() == "true")
    }

    /// The commits that cut a shallow clone's history, sorted; empty in a full clone. A fetch
    /// that deepens or unshallows changes this list while HEAD stays put.
    pub fn shallow_roots(&self) -> Result<Vec<String>, GitError> {
        let path = self.run(&["rev-parse", "--git-path", "shallow"])?;
        let file = self.dir.join(path.trim());
        let mut roots: Vec<String> = match std::fs::read_to_string(&file) {
            Ok(text) => text
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => {
                return Err(GitError::Command {
                    args: "rev-parse --git-path shallow".to_string(),
                    stderr: format!("cannot read {}: {err}", file.display()),
                });
            }
        };
        roots.sort();
        Ok(roots)
    }

    pub(crate) fn run(&self, args: &[&str]) -> Result<String, GitError> {
        let out = self.exec(args, None)?;
        if out.code == 0 {
            Ok(out.stdout)
        } else {
            Err(command_error(args, &out))
        }
    }

    pub(crate) fn run_with_input(&self, args: &[&str], input: &str) -> Result<String, GitError> {
        let out = self.exec(args, Some(input))?;
        if out.code == 0 {
            Ok(out.stdout)
        } else {
            Err(command_error(args, &out))
        }
    }

    /// Runs git and returns its output whatever the exit code, except that a
    /// folder git cannot work in is `Unavailable`.
    pub(crate) fn exec(&self, args: &[&str], input: Option<&str>) -> Result<Output, GitError> {
        check_version()?;
        let mut cmd = Command::new("git");
        cmd.arg("--no-pager")
            .arg("-C")
            .arg(&self.dir)
            .args(["-c", "core.quotepath=off"])
            .args(args)
            .env("LC_ALL", "C")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().map_err(|err| spawn_error(args, &err))?;

        let writer = input.map(|text| {
            let mut stdin = child.stdin.take();
            let text = text.to_string();
            thread::spawn(move || {
                if let Some(stdin) = stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
            })
        });
        let stdout = reader(child.stdout.take());
        let stderr = reader(child.stderr.take());

        let code = match wait_timeout(&mut child, TIMEOUT) {
            Some(code) => code,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Command {
                    args: args.join(" "),
                    stderr: "timed out after 30 s".to_string(),
                });
            }
        };
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        let out = Output {
            code,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        };
        if code != 0 && is_not_a_repo(&out.stderr) {
            return Err(GitError::Unavailable);
        }
        Ok(out)
    }
}

pub(crate) fn command_error(args: &[&str], out: &Output) -> GitError {
    GitError::Command {
        args: args.join(" "),
        stderr: out.stderr.trim().to_string(),
    }
}

fn is_not_a_repo(stderr: &str) -> bool {
    stderr.contains("not a git repository") || stderr.contains("cannot change to")
}

fn spawn_error(args: &[&str], err: &std::io::Error) -> GitError {
    if err.kind() == std::io::ErrorKind::NotFound {
        GitError::Unavailable
    } else {
        GitError::Command {
            args: args.join(" "),
            stderr: err.to_string(),
        }
    }
}

/// Reads a pipe to the end on its own thread, so a large output never blocks git.
fn reader<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_timeout(child: &mut Child, limit: Duration) -> Option<i32> {
    let start = Instant::now();
    let mut pause = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
            Ok(None) => {}
            Err(_) => return Some(-1),
        }
        if start.elapsed() >= limit {
            return None;
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(20));
    }
}

/// `git --version` once per process; a missing git is `Unavailable`. A version
/// that does not parse counts as new.
fn version() -> Result<Option<(u32, u32)>, GitError> {
    static CHECKED: OnceLock<Result<Option<(u32, u32)>, GitError>> = OnceLock::new();
    CHECKED
        .get_or_init(|| {
            let out = Command::new("git")
                .arg("--version")
                .env("LC_ALL", "C")
                .output()
                .map_err(|err| spawn_error(&["--version"], &err))?;
            let text = String::from_utf8_lossy(&out.stdout);
            match parse_version(&text) {
                Some(version) if version < (2, 30) => Err(GitError::TooOld),
                other => Ok(other),
            }
        })
        .clone()
}

fn check_version() -> Result<(), GitError> {
    version().map(drop)
}

/// Whether the installed git is `major.minor` or newer.
pub(crate) fn git_at_least(major: u32, minor: u32) -> Result<bool, GitError> {
    Ok(version()?.is_none_or(|v| v >= (major, minor)))
}

/// `git version 2.43.0.windows.1` to `(2, 43)`.
fn parse_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(2)?;
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}
