//! Recall, precision and exact share of `stone code uses` per language over the
//! synthetic fixtures in `bench/code/<lang>/src`, against the hand-written
//! `expected_uses.yml` next to each.
//!
//! PENDING USER INPUT: the real-sample recall gate (open-source or Freightline
//! samples with expectations captured from an IDE's find-usages, per language)
//! is not part of this harness; these fixtures only keep the harness honest.
//!
//! Run with `cargo test -p carvestone-engine --test bench_code -- --nocapture`
//! to see the numbers.
#![allow(clippy::print_stdout)]

#[path = "it/common/mod.rs"]
mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use carvestone_engine::Repo;
use carvestone_engine::code::{CodeQuery, uses};
use common::Tmp;
use serde::Deserialize;

const LANGS: [&str; 4] = ["csharp", "rust", "typescript", "python"];
const MIN_RECALL: f64 = 0.9;

#[derive(Debug, Deserialize)]
struct Expected {
    symbol: String,
    /// `path:line` relative to the fixture's `src/`, forward slashes.
    sites: Vec<String>,
}

fn lang_dir(lang: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bench")
        .join("code")
        .join(lang)
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

#[derive(Default)]
struct Score {
    expected: usize,
    found: usize,
    matched: usize,
    exact: usize,
}

impl Score {
    fn recall(&self) -> f64 {
        self.matched as f64 / self.expected.max(1) as f64
    }
    fn precision(&self) -> f64 {
        self.matched as f64 / self.found.max(1) as f64
    }
    fn exact_share(&self) -> f64 {
        self.exact as f64 / self.found.max(1) as f64
    }
}

/// The line of an edge hit; `uses` prints it as `@L<line> <signature>`.
fn site_of(hit: &carvestone_engine::search::Hit) -> String {
    let line = hit
        .snippet
        .strip_prefix("@L")
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no site line in {:?}", hit.snippet));
    let path = hit.path.strip_prefix("src/").unwrap_or(&hit.path);
    format!("{path}:{line}")
}

fn score(lang: &str, dir: &Path) -> Score {
    let expected_path = dir.join("expected_uses.yml");
    assert!(expected_path.is_file(), "{lang}: missing {}", expected_path.display());
    let expected: Vec<Expected> = serde_norway::from_str(&fs::read_to_string(&expected_path).unwrap())
        .unwrap_or_else(|err| panic!("{lang}: bad expected_uses.yml: {err}"));

    let t = Tmp::new(&format!("bench-code-{lang}"));
    t.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    copy_dir(&dir.join("src"), &t.0.join("src"));
    let repo = Repo::open(&t.0).unwrap();

    let mut score = Score::default();
    for item in &expected {
        let result = uses(
            &repo,
            &CodeQuery {
                target: item.symbol.clone(),
                limit: 1000,
                ..CodeQuery::default()
            },
        )
        .unwrap_or_else(|err| panic!("{lang}: uses {}: {err}", item.symbol));
        assert_eq!(result.note, None, "{lang}: index still building");
        let want: BTreeSet<&str> = item.sites.iter().map(String::as_str).collect();
        let mut got: BTreeSet<String> = BTreeSet::new();
        let mut exact: BTreeSet<String> = BTreeSet::new();
        for hit in &result.hits {
            let site = site_of(hit);
            if hit.exactness.as_deref() == Some("exact") {
                exact.insert(site.clone());
            }
            got.insert(site);
        }
        score.expected += want.len();
        score.found += got.len();
        score.matched += got.iter().filter(|s| want.contains(s.as_str())).count();
        score.exact += exact.len();
        let missing: Vec<&&str> = want.iter().filter(|s| !got.contains(**s)).collect();
        if !missing.is_empty() {
            println!("{lang}: {} misses {missing:?}; got {got:?}", item.symbol);
        }
    }
    score
}

#[test]
fn uses_recall_per_language() {
    let mut failed = Vec::new();
    for lang in LANGS {
        let s = score(lang, &lang_dir(lang));
        println!(
            "{lang:<10} recall {:.2}  precision {:.2}  exact {:.2}  ({} expected, {} found)",
            s.recall(),
            s.precision(),
            s.exact_share(),
            s.expected,
            s.found
        );
        if s.expected == 0 || s.recall() < MIN_RECALL {
            failed.push(format!("{lang} recall {:.2}", s.recall()));
        }
    }
    assert!(failed.is_empty(), "below {MIN_RECALL}: {failed:?}");
}

#[test]
#[should_panic(expected = "ruby: missing")]
fn missing_expectations_name_the_language() {
    let t = Tmp::new("bench-code-missing");
    score("ruby", &t.0);
}
