//! Items: create, edit, link, rename, renumber, delete and safe saves, through the public `Repo` API.

use std::thread;

use carvestone_engine::create::{NewItem, timestamp_of};
use carvestone_engine::edit::Edit;
use carvestone_engine::query::GetResult;
use carvestone_engine::{EngineError, ErrorKind};

use crate::support::Store;

const TASK: &str = "---\r\ntype: task\r\ntitle: First\r\nstatus: todo\r\npriority: high\r\n---\r\n\r\n## Description\r\n\r\nText.\r\n\r\n## Notes\r\n\r\nlast\r\n";
const BARE: &str = "---\ntype: task\ntitle: Bare\nstatus: todo\n---\n";

fn new(type_name: &str, title: &str, slug: Option<&str>) -> NewItem {
    NewItem {
        type_name: type_name.into(),
        title: title.into(),
        slug: slug.map(Into::into),
        ..NewItem::default()
    }
}

fn set(key: &str, value: &str) -> Edit {
    Edit {
        set: vec![(key.into(), value.into())],
        ..Edit::default()
    }
}

fn section(name: &str, text: &str) -> Edit {
    Edit {
        section: Some((name.into(), text.into())),
        ..Edit::default()
    }
}

/// The text without its `generated:` line, whose date changes every day.
fn unstamped(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| !line.starts_with("generated:"))
        .collect()
}

#[test]
fn each_type_gets_its_id_and_folder() {
    let store = Store::new();
    let repo = store.repo();
    let cases = [
        ("task", "Fix it", None, "CS-1", "stone/tasks/CS-1.md"),
        ("milestone", "First", None, "M-1", "stone/milestones/M-1.md"),
        ("decision", "Pick", None, "ADR-1", "stone/decisions/ADR-1.md"),
        (
            "doc",
            "Setup Guide",
            None,
            "docs/setup-guide",
            "stone/docs/setup-guide.md",
        ),
        ("plan", "Cutover", None, "plans/cutover", "stone/plans/cutover.md"),
        (
            "reference",
            "Rust Book",
            None,
            "refs/rust-book",
            "stone/refs/rust-book.md",
        ),
        ("system", "Acme", Some("acme"), "arch/acme", "stone/arch/acme.md"),
        (
            "container",
            "Api",
            Some("acme/api"),
            "arch/acme/api",
            "stone/arch/acme/api.md",
        ),
        (
            "component",
            "Auth",
            Some("acme/api/auth"),
            "arch/acme/api/auth",
            "stone/arch/acme/api/auth.md",
        ),
    ];
    for (type_name, title, slug, id, path) in cases {
        let made = repo.create(&new(type_name, title, slug), false).unwrap();
        assert_eq!((made.id.as_str(), made.path.as_str()), (id, path), "{type_name}");
        let text = store.read(path);
        assert!(text.starts_with(&format!("---\ntype: {type_name}\ntitle: {title}\n")));
        let stamp = text
            .lines()
            .find_map(|line| line.strip_prefix("created: "))
            .unwrap_or_else(|| panic!("{type_name} has no created line: {text}"));
        assert_eq!(stamp.len(), 25, "{type_name}: {stamp}");
        assert_eq!(stamp.as_bytes()[10], b'T', "{type_name}: {stamp}");
        assert!(stamp.ends_with("+00:00"), "{type_name}: {stamp}");
        assert!(text.contains(&format!("at: '{stamp}' }}")), "{type_name}: {text}");
    }
}

#[test]
fn a_timestamp_is_utc_with_an_offset() {
    let cases = [
        (0, "1970-01-01T00:00:00+00:00"),
        (951_868_799, "2000-02-29T23:59:59+00:00"),
        (1_794_391_402, "2026-11-11T10:03:22+00:00"),
    ];
    for (secs, want) in cases {
        assert_eq!(timestamp_of(secs), want, "{secs}");
    }
}

#[test]
fn an_edit_changes_only_its_lines_and_keeps_crlf() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", TASK);
    let repo = store.repo();

    repo.edit("CS-1", &set("priority", "low"), false).unwrap();
    assert_eq!(
        store.read("stone/tasks/CS-1.md"),
        TASK.replace("priority: high", "priority: low")
    );

    repo.edit("CS-1", &section("Notes", "new"), false).unwrap();
    let want = TASK.replace("priority: high", "priority: low").replace("last", "new");
    let edited = store.read("stone/tasks/CS-1.md");
    assert_eq!(unstamped(&edited), want);
    assert!(
        edited
            .lines()
            .any(|line| line.starts_with("generated: ") && line.contains("+00:00' }")),
        "{edited}"
    );

    // A repeat changes nothing, so it does not stamp a new second.
    repo.edit("CS-1", &section("Notes", "new"), false).unwrap();
    assert_eq!(store.read("stone/tasks/CS-1.md"), edited);
}

#[test]
fn the_index_follows_every_write() {
    const HEAD: &str = "---\nokf_version: \"0.2\"\n---\n\n# Docs\n\n";
    let store = Store::new();
    let repo = store.repo();
    let index = || store.read("stone/index.md");

    let guide = NewItem {
        set: vec![("description".into(), "How to install.".into())],
        ..new("doc", "Guide", Some("guide"))
    };
    repo.create(&guide, false).unwrap();
    assert_eq!(index(), format!("{HEAD}- [Guide](docs/guide.md) - How to install.\n"));

    store.write("stone/tasks/CS-2.md", BARE);
    repo.edit("CS-2", &set("priority", "high"), false).unwrap();
    let both =
        format!("{HEAD}- [Guide](docs/guide.md) - How to install.\n\n# Tasks\n\n- [CS-2: Bare](tasks/CS-2.md)\n");
    assert_eq!(index(), both);

    repo.edit("CS-2", &set("title", "Renamed [x]"), false).unwrap();
    let renamed = both.replace("Bare", "Renamed (x)");
    assert_eq!(index(), renamed);

    repo.edit("CS-2", &set("title", "Renamed [x]"), false).unwrap();
    assert_eq!(index(), renamed);

    repo.rename_item("docs/guide", "docs/setup", false).unwrap();
    let moved = renamed.replace("docs/guide.md", "docs/setup.md");
    assert_eq!(index(), moved);

    repo.delete_item("CS-2", false, false).unwrap();
    let gone = format!("{HEAD}- [Guide](docs/setup.md) - How to install.\n\n# Tasks\n\n");
    assert_eq!(index(), gone);

    repo.edit("docs/setup", &set("title", "Other"), true).unwrap();
    assert_eq!(index(), gone);
}

#[test]
fn stone_index_writes_the_full_index_and_keeps_other_lines() {
    let store = Store::new();
    store.write(
        "stone/docs/b.md",
        "---\ntype: doc\ntitle: Bee\ndescription: Second.\n---\n",
    );
    store.write(
        "stone/docs/a.md",
        "---\ntype: doc\ntitle: Ay\n---\n\n## Summary\n\nShort text.\n",
    );
    store.write(
        "stone/tasks/CS-10.md",
        "---\ntype: task\ntitle: Ten\nstatus: todo\n---\n",
    );
    store.write(
        "stone/tasks/CS-9.md",
        "---\ntype: task\ntitle: Nine\nstatus: todo\n---\n",
    );
    store.write(
        "stone/decisions/ADR-1.md",
        "---\ntype: decision\ntitle: Pick\nstatus: accepted\n---\n",
    );
    store.write("stone/arch/acme.md", "---\ntype: system\ntitle: Acme\n---\n");
    store.write(
        "stone/index.md",
        "---\r\nokf_version: \"0.2\"\r\n---\r\n\r\n# Notes\r\n\r\n- [Site](https://example.com)\r\n- [Tasks](tasks/)\r\n- [Old](docs/gone.md)\r\n",
    );
    let repo = store.repo();

    let first = repo.write_index(false).unwrap();
    let want = "---\r\nokf_version: \"0.2\"\r\n---\r\n\r\n# Notes\r\n\r\n- [Site](https://example.com)\r\n- [Tasks](tasks/)\r\n\r\n# Docs\r\n\r\n- [Ay](docs/a.md) - Short text.\r\n- [Bee](docs/b.md) - Second.\r\n\r\n# Architecture\r\n\r\n- [Acme](arch/acme.md)\r\n\r\n# Decisions\r\n\r\n- [ADR-1: Pick](decisions/ADR-1.md)\r\n\r\n# Tasks\r\n\r\n- [CS-9: Nine](tasks/CS-9.md)\r\n- [CS-10: Ten](tasks/CS-10.md)\r\n";
    assert!(first.changed);
    assert_eq!(store.read("stone/index.md"), want);

    let again = repo.write_index(false).unwrap();
    assert!(!again.changed);
    assert_eq!(store.read("stone/index.md"), want);

    let found = |repo: &carvestone_engine::Repo| -> Vec<String> {
        repo.check().unwrap().iter().map(ToString::to_string).collect()
    };
    assert_eq!(found(&repo), Vec::<String>::new());

    store.write(
        "stone/tasks/CS-9.md",
        "---\ntype: task\ntitle: Nine again\nstatus: todo\n---\n",
    );
    assert_eq!(
        found(&repo),
        ["stone/index.md:1: index: the index is out of date for stone/tasks/CS-9.md; run `stone index`"]
    );
}

#[test]
fn the_section_body_edits_the_text_before_the_first_heading() {
    let store = Store::new();
    let repo = store.repo();

    // (a) no heading at all: the new text is the whole body.
    store.write(
        "stone/milestones/M-1.md",
        "---\ntype: milestone\ntitle: First\n---\n\nOld text.\n",
    );
    repo.edit("M-1", &section("body", "New text.\nTwo lines."), false)
        .unwrap();
    assert_eq!(
        unstamped(&store.read("stone/milestones/M-1.md")),
        "---\ntype: milestone\ntitle: First\n---\n\nNew text.\nTwo lines.\n"
    );

    // (b) text above `## Notes`: only that text changes, the sections below stay, CRLF stays.
    let above = TASK.replace("\r\n## Description", "\r\nIntro.\r\n\r\n## Description");
    store.write("stone/tasks/CS-1.md", &above);
    repo.edit("CS-1", &section("BODY", "Fresh intro."), false).unwrap();
    assert_eq!(
        unstamped(&store.read("stone/tasks/CS-1.md")),
        above.replace("Intro.", "Fresh intro.")
    );

    // (c) an item with its own `## Body` heading: that section changes, the text above stays.
    let own = "---\ntype: task\ntitle: Own\nstatus: todo\n---\n\nIntro.\n\n## Body\n\nOld.\n\n## Notes\n\nlast\n";
    store.write("stone/tasks/CS-2.md", own);
    repo.edit("CS-2", &section("body", "New."), false).unwrap();
    assert_eq!(
        unstamped(&store.read("stone/tasks/CS-2.md")),
        own.replace("Old.", "New.")
    );

    // The same text again changes nothing.
    assert!(
        !repo
            .edit("CS-1", &section("body", "Fresh intro."), false)
            .unwrap()
            .changed
    );
}

#[test]
fn a_section_edit_refuses_to_drop_nested_sections() {
    // Top holds One (with Deep) and Two, so replacing any of them drops the headings below.
    const NESTED: &str =
        "---\ntype: doc\ntitle: Guide\n---\n\n# Top\n\nIntro.\n\n## One\n\nA.\n\n### Deep\n\nD.\n\n## Two\n\nB.\n";
    let store = Store::new();
    let repo = store.repo();
    let rows: [(&str, &str, bool, Result<String, String>); 5] = [
        (
            "Top",
            "New intro.",
            false,
            Err("section `Top` holds 3 sub-sections the new text does not carry (One, Deep, Two); edit each by name, or pass --replace-nested to replace them all".into()),
        ),
        (
            "one",
            "New A.",
            false,
            Err("section `One` holds 1 sub-section the new text does not carry (Deep); edit each by name, or pass --replace-nested to replace them all".into()),
        ),
        (
            "One",
            "New A.\n\n### deep\n\nNew D.",
            false,
            Ok(NESTED.replace("A.\n\n### Deep\n\nD.", "New A.\n\n### deep\n\nNew D.")),
        ),
        ("One", "New A.", true, Ok(NESTED.replace("A.\n\n### Deep\n\nD.\n", "New A.\n"))),
        ("Two", "New B.", false, Ok(NESTED.replace("B.", "New B."))),
    ];
    for (name, text, replace_nested, want) in rows {
        store.write("stone/docs/guide.md", NESTED);
        let req = Edit {
            section: Some((name.into(), text.into())),
            replace_nested,
            ..Edit::default()
        };
        match want {
            Ok(want) => {
                let written = repo
                    .edit("docs/guide", &req, false)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(written.changed, "{name}: the edit writes");
                assert_eq!(unstamped(&store.read("stone/docs/guide.md")), want, "{name}: {text}");
            }
            Err(want) => {
                let err = repo
                    .edit("docs/guide", &req, false)
                    .err()
                    .unwrap_or_else(|| panic!("{name}: edit passed"));
                assert_eq!(err.to_string(), want, "{name}: {text}");
                assert_eq!(store.read("stone/docs/guide.md"), NESTED, "{name}: nothing was written");
            }
        }
    }

    // A dry run refuses the same edit and changes nothing.
    store.write("stone/docs/guide.md", NESTED);
    let dry = repo
        .edit("docs/guide", &section("Top", "New intro."), true)
        .unwrap_err();
    assert_eq!(
        dry.to_string(),
        "section `Top` holds 3 sub-sections the new text does not carry (One, Deep, Two); edit each by name, or pass --replace-nested to replace them all"
    );
    assert_eq!(store.read("stone/docs/guide.md"), NESTED);
}

#[test]
fn a_section_edit_refuses_a_text_that_repeats_the_heading() {
    let store = Store::new();
    let repo = store.repo();
    let rows: [(&str, &str, &str, bool); 7] = [
        ("the heading as sent", "description", "## Description\n\nNew.", true),
        ("another case and level", "description", "# description\n\nNew.", true),
        ("after a blank line", "description", "\n\n## Description\n\nNew.", true),
        ("found by anchor", "notes", "## Notes\n\nNew.", true),
        ("a different heading", "description", "## Other\n\nNew.", false),
        (
            "a sub-heading in the middle",
            "description",
            "New.\n\n### Description\n\nMore.",
            false,
        ),
        ("plain text", "description", "New.", false),
    ];

    store.write("stone/tasks/CS-1.md", TASK);
    for (what, name, text, refused) in rows {
        let result = repo.edit("CS-1", &section(name, text), false);
        if refused {
            let err = result.err().unwrap_or_else(|| panic!("{what}: the edit passed"));
            assert!(err.to_string().contains("starts with its own heading"), "{what}: {err}");
            assert_eq!(store.read("stone/tasks/CS-1.md"), TASK, "{what}: nothing was written");
        } else {
            result.unwrap_or_else(|e| panic!("{what}: {e}"));
            store.write("stone/tasks/CS-1.md", TASK);
        }
    }
}

#[test]
fn the_same_edit_twice_gives_no_diff() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", TASK);
    let repo = store.repo();
    for edit in [set("priority", "low"), section("Notes", "new")] {
        assert!(repo.edit("CS-1", &edit, false).unwrap().changed);
        let once = store.read("stone/tasks/CS-1.md");
        assert!(!repo.edit("CS-1", &edit, false).unwrap().changed);
        assert_eq!(store.read("stone/tasks/CS-1.md"), once);
    }
}

#[test]
fn a_link_shows_as_a_backlink() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", BARE);
    store.write("stone/tasks/CS-2.md", BARE);
    let repo = store.repo();
    repo.edit("CS-2", &set("depends_on", "+CS-1"), false).unwrap();

    let GetResult::Item(view) = repo.get("CS-1").unwrap() else {
        panic!("CS-1 is an item");
    };
    let from: Vec<&str> = view.backlinks.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(from, ["stone/tasks/CS-2.md"]);
}

#[test]
fn a_rename_updates_every_link() {
    let store = Store::new();
    store.write("stone/docs/old.md", "---\ntype: doc\ntitle: Old\n---\n");
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: A\nstatus: todo\nlinks: [docs/old]\n---\n\nSee [[docs/old]] here.\n",
    );
    let renamed = store.repo().rename_item("docs/old", "docs/new", false).unwrap();

    assert_eq!(renamed.to, "stone/docs/new.md");
    assert!(!store.exists("stone/docs/old.md") && store.exists("stone/docs/new.md"));
    assert_eq!(
        store.read("stone/tasks/CS-1.md"),
        "---\ntype: task\ntitle: A\nstatus: todo\nlinks: [docs/new]\n---\n\nSee [[docs/new]] here.\n"
    );
}

#[test]
fn a_renumber_updates_every_link() {
    let store = Store::new();
    store.write("stone/tasks/CS-07.md", BARE);
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: A\nstatus: todo\ndepends_on: [CS-7]\n---\n\nAfter [[CS-7]] and `[[CS-7]]`.\n",
    );
    let renumbered = store.repo().renumber("stone/tasks/CS-07.md", false).unwrap();

    assert_eq!(renumbered.to, "stone/tasks/CS-8.md");
    assert!(!store.exists("stone/tasks/CS-07.md") && store.exists("stone/tasks/CS-8.md"));
    assert_eq!(
        store.read("stone/tasks/CS-1.md"),
        "---\ntype: task\ntitle: A\nstatus: todo\ndepends_on: [CS-8]\n---\n\nAfter [[CS-8]] and `[[CS-7]]`.\n"
    );
}

#[test]
fn a_delete_is_refused_while_an_item_links_to_it() {
    let store = Store::new();
    store.write("stone/docs/spec.md", "---\ntype: doc\ntitle: Spec\n---\n");
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: A\nstatus: todo\nlinks: [docs/spec]\n---\n",
    );
    let repo = store.repo();

    let err = repo.delete_item("docs/spec", false, false).unwrap_err().to_string();
    assert!(err.contains("is linked from CS-1"), "{err}");
    assert!(store.exists("stone/docs/spec.md"));

    repo.edit("CS-1", &set("links", ""), false).unwrap();
    repo.delete_item("docs/spec", false, false).unwrap();
    assert!(!store.exists("stone/docs/spec.md"));
}

#[test]
fn concurrent_writes_all_land() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", BARE);
    let repo = store.repo();

    thread::scope(|s| {
        for n in 0..8 {
            let repo = &repo;
            s.spawn(move || repo.create(&new("task", &format!("T{n}"), None), false).unwrap());
        }
    });
    assert!((2..=9).all(|n| store.exists(&format!("stone/tasks/CS-{n}.md"))));
    assert!(!store.exists("stone/tasks/CS-10.md"));

    thread::scope(|s| {
        for n in 2..=9 {
            let repo = &repo;
            s.spawn(move || {
                repo.edit("CS-1", &set("depends_on", &format!("+CS-{n}")), false)
                    .unwrap()
            });
        }
    });
    let text = store.read("stone/tasks/CS-1.md");
    let line = text.lines().find(|l| l.starts_with("depends_on: [")).unwrap();
    let mut listed: Vec<&str> = line["depends_on: [".len()..line.len() - 1].split(", ").collect();
    listed.sort_unstable();
    assert_eq!(listed, ["CS-2", "CS-3", "CS-4", "CS-5", "CS-6", "CS-7", "CS-8", "CS-9"]);
}

#[test]
fn a_save_never_overwrites_a_newer_or_unreadable_file() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", BARE);
    store.write("stone/tasks/CS-2.md", BARE);
    let repo = store.repo();

    let stale = repo.etag("CS-1").unwrap();
    repo.edit("CS-1", &set("priority", "low"), false).unwrap();
    let kept = store.read("stone/tasks/CS-1.md");
    let err = repo
        .edit_if_match("CS-1", &set("priority", "high"), false, Some(&stale))
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert_eq!(store.read("stone/tasks/CS-1.md"), kept);

    let bad = b"---\ntype: task\ntitle: Caf\xe9\nstatus: todo\n---\n";
    std::fs::write(store.path("stone/tasks/CS-2.md"), bad).unwrap();
    let err = repo
        .edit("CS-2", &set("priority", "low"), false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not valid UTF-8"), "{err}");
    assert_eq!(std::fs::read(store.path("stone/tasks/CS-2.md")).unwrap(), bad);
}

#[test]
fn errors() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", BARE);
    let repo = store.repo();
    let cases: [(&str, EngineError, &str); 24] = [
        (
            "bad type",
            repo.create(&new("bogus", "x", None), false).unwrap_err(),
            "unknown type `bogus`",
        ),
        (
            "missing title",
            repo.create(&new("task", " ", None), false).unwrap_err(),
            "a title is required",
        ),
        (
            "unknown id",
            repo.edit("CS-99", &set("priority", "low"), false).unwrap_err(),
            "CS-99 not found",
        ),
        (
            "bad field",
            repo.edit("CS-1", &set("colour", "red"), false).unwrap_err(),
            "`colour`: it is not a field stone knows",
        ),
        (
            "bad status",
            repo.edit("CS-1", &set("status", "bogus"), false).unwrap_err(),
            "status `bogus` is not valid for a task",
        ),
        (
            "bad stale_after",
            repo.edit("CS-1", &set("stale_after", "soon"), false).unwrap_err(),
            "stale_after `soon` is not a date",
        ),
        (
            "impossible stale_after day",
            repo.edit("CS-1", &set("stale_after", "2026-13-45"), false).unwrap_err(),
            "stale_after `2026-13-45` is not a date",
        ),
        (
            "relative stale_after",
            repo.edit("CS-1", &set("stale_after", "5d"), false).unwrap_err(),
            "stale_after `5d` is not a date",
        ),
        (
            "stale_after with a space before the time",
            repo.edit("CS-1", &set("stale_after", "2026-12-01 09:30"), false)
                .unwrap_err(),
            "stale_after `2026-12-01 09:30` is not a date",
        ),
        (
            "empty stale_after",
            repo.edit("CS-1", &set("stale_after", ""), false).unwrap_err(),
            "stale_after `` is not a date",
        ),
        (
            "stale_after with a word for a time",
            repo.edit("CS-1", &set("stale_after", "2026-12-01Tsoon"), false)
                .unwrap_err(),
            "stale_after `2026-12-01Tsoon` is not a date",
        ),
        (
            "stale_after time without an offset",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30:00"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30:00` is not a date",
        ),
        (
            "stale_after time without seconds",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30+00:00"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30+00:00` is not a date",
        ),
        (
            "stale_after hour 24",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T24:00:00+00:00"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T24:00:00+00:00` is not a date",
        ),
        (
            "stale_after second 60",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30:60+00:00"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30:60+00:00` is not a date",
        ),
        (
            "stale_after offset without a colon",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30:00+0000"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30:00+0000` is not a date",
        ),
        (
            "stale_after fraction without digits",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30:00.+00:00"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30:00.+00:00` is not a date",
        ),
        (
            "stale_after text after the offset",
            repo.edit("CS-1", &set("stale_after", "2026-12-01T09:30:00+00:00x"), false)
                .unwrap_err(),
            "stale_after `2026-12-01T09:30:00+00:00x` is not a date",
        ),
        (
            "bad created",
            repo.edit("CS-1", &set("created", "soon"), false).unwrap_err(),
            "created `soon` is not a date",
        ),
        (
            "empty created",
            repo.edit("CS-1", &set("created", ""), false).unwrap_err(),
            "created `` is not a date",
        ),
        (
            "created time without an offset",
            repo.edit("CS-1", &set("created", "2026-12-01T09:30:00"), false)
                .unwrap_err(),
            "created `2026-12-01T09:30:00` is not a date",
        ),
        (
            "new item with bad stale_after",
            repo.create(
                &NewItem {
                    set: vec![("stale_after".into(), "soon".into())],
                    ..new("doc", "Dated", None)
                },
                false,
            )
            .unwrap_err(),
            "stale_after `soon` is not a date",
        ),
        (
            "new item with bad created",
            repo.create(
                &NewItem {
                    set: vec![("created".into(), "soon".into())],
                    ..new("doc", "Born", None)
                },
                false,
            )
            .unwrap_err(),
            "created `soon` is not a date",
        ),
        (
            "new item from markdown with bad stale_after",
            repo.create(
                &NewItem::from_markdown("doc", "---\ntitle: Filed\nstale_after: soon\n---\n\nText.\n", "filed")
                    .unwrap(),
                false,
            )
            .unwrap_err(),
            "stale_after `soon` is not a date",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
    assert_eq!(store.read("stone/tasks/CS-1.md"), BARE);
    assert!(!store.exists("stone/tasks/CS-2.md"));
    assert!(!store.exists("stone/docs/dated.md"));
    assert!(!store.exists("stone/docs/born.md"));
    assert!(!store.exists("stone/docs/filed.md"));
}

#[test]
fn from_markdown_keeps_stale_after_and_names_what_it_left_out() {
    let store = Store::new();
    let repo = store.repo();
    let cases = [
        (
            "dated, with foreign fields",
            "---\ntype: doc\ntitle: Probe\nstale_after: 2026-01-01\nowner: me\nstatus: draft\n---\n\n# Probe\n",
            Some("stale_after: 2026-01-01"),
            vec![
                "left out front matter fields: owner, status; a file keeps only title, description, tags, links and stale_after, so set the others with --set",
            ],
        ),
        ("title only", "---\ntitle: Plain\n---\n\nText.\n", None, vec![]),
        (
            "timestamp",
            "---\ntitle: Stamped\nstale_after: 2026-01-01T09:30:00+00:00\n---\n\nText.\n",
            Some("stale_after: 2026-01-01T09:30:00+00:00"),
            vec![],
        ),
        (
            "zulu timestamp",
            "---\ntitle: Zulu\nstale_after: 2026-01-01T09:30:00Z\n---\n\nText.\n",
            Some("stale_after: 2026-01-01T09:30:00Z"),
            vec![],
        ),
        (
            "fraction and negative offset",
            "---\ntitle: Fraction\nstale_after: 2026-01-01T09:30:00.250-08:00\n---\n\nText.\n",
            Some("stale_after: 2026-01-01T09:30:00.250-08:00"),
            vec![],
        ),
        (
            "list stale_after",
            "---\ntitle: Listed date\nstale_after: [2026-01-01]\n---\n\nText.\n",
            None,
            vec![
                "left out front matter fields: stale_after; a file keeps only title, description, tags, links and stale_after, so set the others with --set",
            ],
        ),
        (
            "list description",
            "---\ntitle: Listed text\ndescription: [a, b]\n---\n\nText.\n",
            None,
            vec![
                "left out front matter fields: description; a file keeps only title, description, tags, links and stale_after, so set the others with --set",
            ],
        ),
        (
            "number key",
            "---\ntitle: Number key\n123: x\n---\n\nText.\n",
            None,
            vec![
                "left out front matter fields: 123; a file keeps only title, description, tags, links and stale_after, so set the others with --set",
            ],
        ),
    ];
    for (what, source, stale_line, want) in cases {
        let req = NewItem::from_markdown("doc", source, "probe").unwrap();
        let dry = repo.create(&req, true).unwrap();
        assert_eq!(dry.warnings, want, "{what}: dry run");
        if let Some(line) = stale_line {
            assert!(dry.text.lines().any(|l| l == line), "{what}: {}", dry.text);
        } else {
            assert!(!dry.text.contains("stale_after"), "{what}: {}", dry.text);
        }
        let made = repo.create(&req, false).unwrap();
        assert_eq!(made.warnings, want, "{what}: create");
        assert_eq!(
            made.text.contains("stale_after: 2026-01-01"),
            stale_line.is_some(),
            "{what}"
        );
    }
}
