//! Temp repos and fixture folders shared by the `stone check` test files.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use carvestone_engine::{Problem, Repo};

static NEXT: AtomicU32 = AtomicU32::new(0);

const CONFIG: &str = "project: t\ntask_prefix: CS\n";
const INDEX: &str = "---\nokf_version: \"0.2\"\n---\n";

/// A temp repo removed on drop, holding a config and a root index.
pub struct Tmp(pub PathBuf);

impl Tmp {
    pub fn new(name: &str) -> Tmp {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-chk-{name}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let t = Tmp(dir);
        t.write("stone/config.yml", CONFIG);
        t.write("stone/index.md", INDEX);
        t
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        fs::remove_file(self.0.join(rel)).unwrap();
    }

    pub fn repo(&self) -> Repo {
        Repo::open(&self.0).unwrap().with_author("claude-code".to_string())
    }

    pub fn check(&self) -> Vec<Problem> {
        Repo::open(&self.0).unwrap().check().unwrap()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A temp repo with `tests/fixtures/<name>` copied over the base files.
pub fn fixture(name: &str) -> Tmp {
    let t = Tmp::new(&name.replace('/', "-"));
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    copy_dir(&src, &t.0);
    t
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// (path, line, rule) of each problem, in report order.
pub fn summary(problems: &[Problem]) -> Vec<(String, usize, &'static str)> {
    problems.iter().map(|p| (p.path.clone(), p.line, p.rule)).collect()
}
