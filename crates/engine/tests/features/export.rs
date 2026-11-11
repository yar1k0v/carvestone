//! `stone export --okf`: relations as standard markdown links in a separate copy.

use std::fs;

use carvestone_engine::ErrorKind;

use crate::support::Store;

const TASK_1: &str = "---\ntype: task\ntitle: Build it\nstatus: todo\nmilestone: M-1\ncomponents: [arch/acme/api/auth]\nlinks: [docs/setup, refs/rust-book]\ndepends_on: [CS-2]\n---\n\n## Description\n\nSee [[docs/setup#overview]] and [[ADR-1]].\n";

const TASK_1_OKF: &str = "---\ntype: task\ntitle: Build it\nstatus: todo\nmilestone: M-1\ncomponents: [arch/acme/api/auth]\nlinks: [docs/setup, refs/rust-book]\ndepends_on: [CS-2]\n---\n\n## Description\n\nSee [docs/setup#overview](../docs/setup.md#overview) and [ADR-1](../decisions/ADR-1.md).\n\n## Links\n\n### links\n\n- [Setup](../docs/setup.md)\n- [Rust Book](../refs/rust-book.md)\n\n### depends_on\n\n- [CS-2: Done one](CS-2.md)\n\n### components\n\n- [Auth](../arch/acme/api/auth.md)\n\n### milestone\n\n- [M-1: First](../milestones/M-1.md)\n";

const CRLF: &str = "---\r\ntype: doc\r\ntitle: Crlf\r\nlinks: [docs/nope]\r\n---\r\n\r\n[[code:crates/a.rs]]\r\n[[docs/nope]]\r\n`[[ADR-1]]`\r\n```\r\n[[ADR-1]]\r\n```\r\n";

const CRLF_OKF: &str = "---\r\ntype: doc\r\ntitle: Crlf\r\nlinks: [docs/nope]\r\n---\r\n\r\n`crates/a.rs`\r\n[[docs/nope]]\r\n`[[ADR-1]]`\r\n```\r\n[[ADR-1]]\r\n```\r\n\r\n## Links\r\n\r\n### links\r\n\r\n- docs/nope\r\n";

/// The files of the worked example, as (path, text).
fn example() -> Vec<(&'static str, &'static str)> {
    vec![
        ("stone/tasks/CS-1.md", TASK_1),
        (
            "stone/tasks/CS-2.md",
            "---\ntype: task\ntitle: Done one\nstatus: done\n---\n",
        ),
        (
            "stone/milestones/M-1.md",
            "---\ntype: milestone\ntitle: First\nstatus: open\n---\n",
        ),
        (
            "stone/decisions/ADR-1.md",
            "---\ntype: decision\ntitle: Pick\nstatus: accepted\n---\n",
        ),
        (
            "stone/docs/setup.md",
            "---\ntype: doc\ntitle: Setup\n---\n\n## Overview\n\nText.\n",
        ),
        ("stone/refs/rust-book.md", "---\ntype: ref\ntitle: Rust Book\n---\n"),
        ("stone/arch/acme.md", "---\ntype: system\ntitle: Acme\n---\n"),
        ("stone/arch/acme/api.md", "---\ntype: container\ntitle: Api\n---\n"),
        (
            "stone/arch/acme/api/auth.md",
            "---\ntype: component\ntitle: Auth\n---\n",
        ),
        (
            "stone/index.md",
            "---\nokf_version: \"0.2\"\n---\n\n# Tasks\n\n- [CS-1: Build it](tasks/CS-1.md)\n",
        ),
    ]
}

#[test]
fn an_export_writes_relations_as_markdown_links() {
    let store = Store::new();
    for (path, text) in example() {
        store.write(path, text);
    }
    let repo = store.repo();

    let out = store.path("out");
    let done = repo.export_okf(&out).unwrap();
    assert_eq!(done.files, 10);
    assert!(done.unresolved.is_empty(), "{:?}", done.unresolved);
    assert_eq!(fs::read_to_string(out.join("tasks/CS-1.md")).unwrap(), TASK_1_OKF);
    for (path, text) in example() {
        let rel = path.strip_prefix("stone/").unwrap();
        if rel != "tasks/CS-1.md" {
            assert_eq!(fs::read_to_string(out.join(rel)).unwrap(), text, "{rel}");
        }
        assert_eq!(store.read(path), text, "the store changed: {path}");
    }

    store.write("stone/docs/crlf.md", CRLF);
    let second = store.path("out2");
    let done = repo.export_okf(&second).unwrap();
    assert_eq!(done.files, 11);
    assert_eq!(fs::read_to_string(second.join("docs/crlf.md")).unwrap(), CRLF_OKF);
    assert_eq!(
        done.unresolved,
        ["stone/docs/crlf.md: [[docs/nope]]", "stone/docs/crlf.md: docs/nope"]
    );
    assert_eq!(store.read("stone/docs/crlf.md"), CRLF);
}

#[test]
fn errors() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", "---\ntype: task\ntitle: A\nstatus: todo\n---\n");
    store.write("taken/keep.txt", "mine");
    let repo = store.repo();
    fs::create_dir_all(store.path(".stone")).unwrap();
    let taken = store.path("taken");
    let not_empty = "is not empty; export into a new or empty folder";
    let in_store = "is inside the store; export into a folder outside stone/ and .stone/";
    let mut cases = vec![
        ("folder with a file", taken.clone(), not_empty),
        ("inside stone/", store.path("stone/out"), in_store),
        ("inside .stone/", store.path(".stone/out"), in_store),
        ("dot-dot into stone/", store.path("taken/../stone/out"), in_store),
        ("dot-dot through a new folder", store.path("stone/new/../out"), in_store),
        ("trailing separator", store.path("stone/out/"), in_store),
    ];
    // Other-case forms only mean the store on a file system that ignores case, so ask the
    // file system with the existing store folders.
    if store.path("STONE").exists() {
        cases.push(("other case of stone/", store.path("STONE/out"), in_store));
    }
    if store.path(".STONE").exists() {
        cases.push(("other case of .stone/", store.path(".STONE/out"), in_store));
    }
    for (what, folder, message) in &cases {
        let err = repo.export_okf(folder).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Invalid, "{what}");
        assert_eq!(err.to_string(), format!("{} {message}", folder.display()), "{what}");
    }
    assert_eq!(fs::read_dir(&taken).unwrap().count(), 1);
    for folder in ["stone/out", "stone/new", ".stone/out"] {
        assert!(!store.path(folder).exists(), "{folder}");
    }
}
