//! A temp store with files written as given, and the repo over it.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use carvestone_engine::Repo;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A temp store with prefix `CS`, removed on drop.
pub struct Store(PathBuf);

impl Store {
    pub fn new() -> Store {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-features-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store(dir);
        store.write("stone/config.yml", "project: t\ntask_prefix: CS\n");
        store
    }

    pub fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    /// Makes the folder a git repo that tracks every file written so far, for what asks git.
    pub fn git_track(&self) {
        for args in [&["init", "-q"][..], &["-c", "core.autocrlf=false", "add", "-A"]] {
            let done = Command::new("git").args(args).current_dir(&self.0).status().unwrap();
            assert!(done.success(), "git {args:?}");
        }
    }

    /// Commits every change so far as author `dev` on the given ISO 8601 time, and returns the sha.
    /// The first call makes the folder a git repo.
    pub fn git_commit(&self, message: &str, date: &str) -> String {
        self.git_commit_in("", message, date)
    }

    /// `git_commit` for the repo in the folder `dir` below the store, for a product of several repos.
    pub fn git_commit_in(&self, dir: &str, message: &str, date: &str) -> String {
        if !self.exists(format!("{dir}/.git").trim_start_matches('/')) {
            self.git(dir, &["init", "-q"], date);
        }
        self.git(dir, &["add", "-A"], date);
        self.git(
            dir,
            &[
                "-c",
                "user.name=dev",
                "-c",
                "user.email=dev@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                message,
            ],
            date,
        );
        self.git(dir, &["rev-parse", "HEAD"], date).trim().to_string()
    }

    fn git(&self, dir: &str, args: &[&str], date: &str) -> String {
        let out = Command::new("git")
            .args(["-c", "core.autocrlf=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .current_dir(self.0.join(dir))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    pub fn repo(&self) -> Repo {
        Repo::open(&self.0).unwrap().with_author("test".into())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
