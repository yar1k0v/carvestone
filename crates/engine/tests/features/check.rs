//! Check: one pass over a whole store, through `Repo::check`.

use crate::support::Store;

const INDEX: &str = "---\nokf_version: \"0.2\"\n---\n";

/// One file of every item type, linked together, with one owned source file.
fn healthy() -> Store {
    let store = Store::new();
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots: [crates/]\n",
    );
    store.write("stone/index.md", INDEX);
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: Build it\nstatus: todo\npriority: high\nkind: feature\ncreated: 2026-11-01\nmilestone: M-1\ncomponents: [arch/acme/api/auth]\nlinks: [docs/setup, refs/rust-book]\ndepends_on: [CS-2]\n---\n\n## Description\n\nSee [[docs/setup#overview]] and [[ADR-1]].\n",
    );
    store.write(
        "stone/tasks/CS-2.md",
        "---\ntype: task\ntitle: Done one\nstatus: done\n---\n\n## Outcome\n\nIt works.\n",
    );
    store.write(
        "stone/milestones/M-1.md",
        "---\ntype: milestone\ntitle: First\nstatus: active\ncreated: 2026-11-01T10:00:00+00:00\ngenerated: { by: claude-code, at: '2026-11-01T10:00:00+00:00' }\n---\n",
    );
    store.write(
        "stone/decisions/ADR-1.md",
        "---\ntype: decision\ntitle: Pick\nstatus: accepted\n---\n",
    );
    store.write(
        "stone/docs/setup.md",
        "---\ntype: doc\ntitle: Setup\nstale_after: 2999-01-01T00:00:00+00:00\n---\n\n## Overview\n\nText.\n",
    );
    store.write(
        "stone/plans/cutover.md",
        "---\ntype: plan\ntitle: Cutover\nstatus: active\n---\n",
    );
    store.write(
        "stone/refs/rust-book.md",
        "---\ntype: reference\ntitle: Rust Book\n---\n",
    );
    store.write("stone/arch/acme.md", "---\ntype: system\ntitle: Acme\n---\n");
    store.write("stone/arch/acme/api.md", "---\ntype: container\ntitle: Api\n---\n");
    store.write(
        "stone/arch/acme/api/auth.md",
        "---\ntype: component\ntitle: Auth\ncode: [crates/auth/**]\n---\n",
    );
    store.write("crates/auth/lib.rs", "pub fn login() {}\n");
    store.git_track();
    store
}

#[test]
fn a_healthy_store_has_no_problems() {
    let store = healthy();
    let found: Vec<String> = store.repo().check().unwrap().iter().map(ToString::to_string).collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn syntax_lists_every_parse_error() {
    let store = healthy();
    for n in 1..=6 {
        store.write(&format!("crates/auth/bad{n}.rs"), "fn (\n");
    }
    store.git_track();

    let repo = store.repo();
    let sites: Vec<String> = repo.syntax_sites().unwrap().iter().map(ToString::to_string).collect();
    for n in 1..=6 {
        let path = format!("crates/auth/bad{n}.rs:1: syntax");
        assert!(
            sites.iter().any(|s| s.starts_with(&path)),
            "{path} missing from {sites:#?}"
        );
    }
    assert_eq!(sites.len(), 6, "{sites:#?}");

    let found: Vec<String> = repo.check().unwrap().iter().map(ToString::to_string).collect();
    let summary: Vec<&String> = found.iter().filter(|s| s.contains(": syntax:")).collect();
    assert_eq!(summary.len(), 1, "{found:#?}");
    assert!(summary[0].contains("1 more"), "{}", summary[0]);
}

#[test]
fn problems() {
    let store = healthy();
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: Build it\nstatus: todo\nlinks: [docs/missing]\n---\n\n## Description\n\nSee [[docs/setup#nope]].\n",
    );
    store.write(
        "stone/tasks/CS-3.md",
        "---\ntype: task\ntitle: Bad\nstatus: bogus\n---\n",
    );
    store.write(
        "stone/tasks/CS-4.md",
        "---\ntype: task\ntitle: Four\nstatus: todo\n---\n",
    );
    store.write(
        "stone/tasks/CS-04.md",
        "---\ntype: task\ntitle: Again\nstatus: todo\n---\n",
    );
    store.write(
        "stone/docs/misfiled.md",
        "---\ntype: task\ntitle: Misfiled\nstatus: todo\n---\n",
    );
    store.write(
        "stone/docs/timed.md",
        "---\ntype: doc\ntitle: Timed\ncreated: 2026-11-01T10:00\n---\n",
    );
    store.write(
        "stone/docs/dated.md",
        "---\ntype: doc\ntitle: Dated\nstale_after: soon\n---\n",
    );
    store.write("crates/stray/lib.rs", "pub fn lost() {}\n");
    store.write("crates/stray/logo.png", "x");
    store.git_track();

    let found: Vec<String> = store.repo().check().unwrap().iter().map(ToString::to_string).collect();
    let want = [
        "crates/stray/lib.rs:1: owner: no arch element owns this file; add it to the code globs of one component in stone/arch/",
        "stone/docs/dated.md:4: date: stale_after `soon` is not a date; use YYYY-MM-DD",
        "stone/docs/misfiled.md:2: folder: a task belongs in stone/tasks/",
        "stone/docs/timed.md:4: date: created `2026-11-01T10:00` is not a date; use YYYY-MM-DD",
        "stone/tasks/CS-04.md:1: duplicate-id: same id as stone/tasks/CS-04.md, stone/tasks/CS-4.md; give one a new number (`stone renumber <path>`) or remove one",
        "stone/tasks/CS-04.md:1: file-name: name the file CS-4.md",
        "stone/tasks/CS-1.md:5: relation: docs/missing not found",
        "stone/tasks/CS-1.md:10: anchor: docs/setup has no section #nope; sections: overview",
        "stone/tasks/CS-3.md:4: status: status `bogus` is not valid for a task; use one of: draft, todo, in-progress, review, done, archived",
        "stone/tasks/CS-4.md:1: duplicate-id: same id as stone/tasks/CS-04.md, stone/tasks/CS-4.md; give one a new number (`stone renumber <path>`) or remove one",
    ];
    assert_eq!(found, want);
}
