//! Tasks: moving a task through its statuses, ticking its criteria and finding the next ready task.

use carvestone_engine::EngineError;
use carvestone_engine::edit::Edit;
use carvestone_engine::query::{GetResult, ListFilter};

use crate::support::Store;

const TASK: &str = "---\ntype: task\ntitle: Ship\nstatus: todo\npriority: high\n---\n\n## Description\n\nText.\n\n## Acceptance criteria\n\n- [ ] one\n- [ ] two\n\n## Outcome\n\nIt works.\n";

/// The text without lines that start with `prefix`, for dates that change every day.
fn without(text: &str, prefix: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| !line.starts_with(prefix))
        .collect()
}

fn task(status: &str, priority: &str, depends_on: &str) -> String {
    format!("---\ntype: task\ntitle: T\nstatus: {status}\npriority: {priority}\ndepends_on: [{depends_on}]\n---\n")
}

#[test]
fn list_rows_carry_what_a_board_card_shows() {
    let store = Store::new();
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: Full\nstatus: todo\ncreated: 2026-11-01\ncomponents: [arch/acme/web/ui, arch/acme/api/auth, arch/acme/api/login]\n---\n\n## Description\n\n- [x] not a criterion\n\n## Acceptance criteria\n\n- [x] one\n- [ ] two\n",
    );
    store.write(
        "stone/tasks/CS-2.md",
        "---\ntype: task\ntitle: Bare\nstatus: todo\n---\n",
    );
    store.write("stone/arch/acme.md", "---\ntype: system\ntitle: Acme\n---\n");
    store.write("stone/arch/acme/api.md", "---\ntype: container\ntitle: Api\n---\n");
    store.write("stone/arch/acme/web.md", "---\ntype: container\ntitle: Web\n---\n");
    for (path, title) in [("api/auth", "Auth"), ("api/login", "Login"), ("web/ui", "Ui")] {
        store.write(
            &format!("stone/arch/acme/{path}.md"),
            &format!("---\ntype: component\ntitle: {title}\n---\n"),
        );
    }

    let filter = ListFilter {
        item_type: Some("task".to_string()),
        ..ListFilter::default()
    };
    let rows = store.repo().list(&filter).unwrap();
    let facts = |n: usize| {
        let row = &rows[n];
        (
            row.created.clone(),
            row.criteria_done,
            row.criteria_total,
            row.services.clone(),
        )
    };
    assert_eq!(
        facts(0),
        (
            Some("2026-11-01".to_string()),
            Some(1),
            Some(2),
            vec!["Web".to_string(), "Api".to_string()]
        )
    );
    assert_eq!(facts(1), (None, None, None, Vec::new()));
}

#[test]
fn the_list_gives_a_plan_its_created_day() {
    let store = Store::new();
    store.write(
        "stone/plans/dated.md",
        "---\ntype: plan\ntitle: Dated\nstatus: active\ncreated: 2026-10-26\n---\n",
    );
    store.write(
        "stone/plans/undated.md",
        "---\ntype: plan\ntitle: Undated\nstatus: draft\n---\n",
    );
    store.write(
        "stone/docs/setup.md",
        "---\ntype: doc\ntitle: Setup\ncreated: 2026-10-26\n---\n",
    );

    let rows = store.repo().list(&ListFilter::default()).unwrap();
    let created = |title: &str| {
        rows.iter()
            .find(|row| row.title.as_deref() == Some(title))
            .unwrap_or_else(|| panic!("{title} is listed"))
            .created
            .clone()
    };
    assert_eq!(created("Dated"), Some("2026-10-26".to_string()));
    assert_eq!(created("Undated"), None);
    assert_eq!(created("Setup"), None);
}

#[test]
fn get_gives_the_criteria_and_outcome_the_card_counts() {
    let store = Store::new();
    store.write(
        "stone/tasks/CS-1.md",
        r#"---
type: task
title: Deep
status: todo
---

## Acceptance criteria

- [x] one
* [ ] star
+ [x] plus

### Edge cases

- [ ] under a sub-heading

````text
```
## x
- [ ] fenced
````

- [ ] after the fence

## Outcome

It works.

### Details

More text.

````text
```
## x
````
"#,
    );
    store.write(
        "stone/tasks/CS-2.md",
        "---\ntype: task\ntitle: Empty\nstatus: todo\n---\n\n## Outcome\n\n\n",
    );
    let repo = store.repo();

    let GetResult::Item(view) = repo.get("CS-1").unwrap() else {
        panic!("CS-1 is an item");
    };
    let boxes: Vec<_> = view
        .criteria
        .iter()
        .map(|c| (c.n, c.text.as_str(), c.checked))
        .collect();
    assert_eq!(
        boxes,
        [
            (1, "one", true),
            (2, "under a sub-heading", false),
            (3, "after the fence", false)
        ]
    );
    assert_eq!(
        view.outcome.as_deref(),
        Some("It works.\n\n### Details\n\nMore text.\n\n````text\n```\n## x\n````")
    );

    let filter = ListFilter {
        item_type: Some("task".to_string()),
        ..ListFilter::default()
    };
    let rows = repo.list(&filter).unwrap();
    assert_eq!((rows[0].criteria_done, rows[0].criteria_total), (Some(1), Some(3)));

    let GetResult::Item(empty) = repo.get("CS-2").unwrap() else {
        panic!("CS-2 is an item");
    };
    assert!(empty.criteria.is_empty());
    assert_eq!(empty.outcome, None);
}

#[test]
fn a_task_moves_from_todo_to_done_and_back() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", TASK);
    let repo = store.repo();

    for status in ["in-progress", "review"] {
        repo.move_task("CS-1", status, true, false).unwrap();
        assert_eq!(
            store.read("stone/tasks/CS-1.md"),
            TASK.replace("status: todo", &format!("status: {status}"))
        );
    }

    repo.move_task("CS-1", "done", true, false).unwrap();
    let text = store.read("stone/tasks/CS-1.md");
    assert!(text.contains("\nclosed: 20"), "{text}");
    // `closed` stays a plain day, never a timestamp.
    let closed = text.lines().find(|line| line.starts_with("closed: ")).unwrap();
    assert_eq!(closed.len(), 18, "{closed}");
    assert_eq!(without(&text, "closed:"), TASK.replace("status: todo", "status: done"));

    repo.move_task("CS-1", "in-progress", true, false).unwrap();
    assert_eq!(
        store.read("stone/tasks/CS-1.md"),
        TASK.replace("status: todo", "status: in-progress")
    );
}

#[test]
fn ticking_a_criterion_changes_only_that_box() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", TASK);
    let repo = store.repo();

    let tick = Edit {
        check_ac: vec![2],
        ..Edit::default()
    };
    repo.edit("CS-1", &tick, false).unwrap();
    assert_eq!(
        without(&store.read("stone/tasks/CS-1.md"), "generated:"),
        TASK.replace("- [ ] two", "- [x] two")
    );

    store.write("stone/tasks/CS-2.md", TASK);
    let both = Edit {
        check_ac: vec![1, 2],
        ..Edit::default()
    };
    repo.edit("CS-2", &both, false).unwrap();
    assert_eq!(
        without(&store.read("stone/tasks/CS-2.md"), "generated:"),
        TASK.replace("- [ ] one", "- [x] one").replace("- [ ] two", "- [x] two")
    );

    store.write("stone/tasks/CS-3.md", TASK);
    let first_and_last = Edit {
        check_ac: vec![1, 3],
        ..Edit::default()
    };
    let err = repo.edit("CS-3", &first_and_last, false).unwrap_err();
    assert!(err.to_string().contains("no. 3 does not exist"), "{err}");
    assert_eq!(store.read("stone/tasks/CS-3.md"), TASK);
}

#[test]
fn a_note_lands_under_notes_once() {
    let day = carvestone_engine::create::today();
    let with_notes = TASK.replace("## Outcome", "## Notes\n\n- 2026-01-01: old\n\n## Outcome");
    let crlf = TASK.replace('\n', "\r\n");
    // (what the row shows, the file, the note, the file after the edit, or the error sentence)
    let rows: [(&str, String, &str, Result<String, &str>); 5] = [
        (
            "no Notes section",
            TASK.to_string(),
            "Sent back: fix the title",
            Ok(format!("{TASK}\n## Notes\n\n- {day}: Sent back: fix the title\n")),
        ),
        (
            "a Notes section with another section after it",
            with_notes.clone(),
            "Sent back: fix the title",
            Ok(with_notes.replace(
                "- 2026-01-01: old\n",
                &format!("- 2026-01-01: old\n- {day}: Sent back: fix the title\n"),
            )),
        ),
        (
            "a CRLF file",
            crlf.clone(),
            "Sent back: fix the title",
            Ok(format!(
                "{crlf}\r\n## Notes\r\n\r\n- {day}: Sent back: fix the title\r\n"
            )),
        ),
        (
            "a comment with a newline and a heading",
            TASK.to_string(),
            "first\n## x\n\nsecond",
            Ok(format!("{TASK}\n## Notes\n\n- {day}: first ## x second\n")),
        ),
        (
            "a blank comment",
            TASK.to_string(),
            " \n\t ",
            Err("the note is empty; write what has to change"),
        ),
    ];
    for (what, file, note, expected) in rows {
        let store = Store::new();
        store.write("stone/tasks/CS-1.md", &file);
        let repo = store.repo();
        let req = Edit {
            note: Some(note.to_string()),
            ..Edit::default()
        };
        match expected {
            Ok(text) => {
                repo.edit("CS-1", &req, false).unwrap();
                let saved = store.read("stone/tasks/CS-1.md");
                assert_eq!(without(&saved, "generated:"), text, "{what}");
                // The same write twice gives no diff.
                let again = repo.edit("CS-1", &req, false).unwrap();
                assert!(!again.changed, "{what}");
                assert_eq!(store.read("stone/tasks/CS-1.md"), saved, "{what}");
            }
            Err(sentence) => {
                let err = repo.edit("CS-1", &req, false).unwrap_err();
                assert_eq!(err.to_string(), sentence, "{what}");
                assert_eq!(store.read("stone/tasks/CS-1.md"), file, "{what}");
            }
        }
    }
}

#[test]
fn next_lists_a_task_only_when_its_dependencies_are_done() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", &TASK.replace("priority: high", "priority: low"));
    store.write("stone/tasks/CS-2.md", &task("todo", "high", "CS-1"));
    store.write("stone/tasks/CS-3.md", &task("todo", "medium", "CS-4"));
    store.write("stone/tasks/CS-4.md", &task("done", "low", ""));
    store.write("stone/tasks/CS-5.md", &task("in-progress", "high", ""));
    store.write("stone/tasks/CS-6.md", &task("todo", "high", "CS-99"));
    store.write("stone/tasks/CS-7.md", &task("todo", "high", "CS-8"));
    store.write("stone/tasks/CS-8.md", &task("review", "low", ""));
    let repo = store.repo();

    let ids = |limit| -> Vec<String> {
        repo.next(limit)
            .unwrap()
            .into_iter()
            .map(|row| row.short.unwrap())
            .collect()
    };
    assert_eq!(ids(5), ["CS-3", "CS-1"]);
    assert_eq!(ids(1), ["CS-3"]);

    repo.move_task("CS-1", "done", true, false).unwrap();
    assert_eq!(ids(5), ["CS-2", "CS-3"]);
}

#[test]
fn the_project_map_lists_parts_review_ready_and_progress() {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    // The derived index is not source; a store made by `init` ignores it the same way.
    store.write(".gitignore", "/.stone/\n");
    store.write(
        "stone/arch/app.md",
        "---\ntype: container\ntitle: App\ncode: ['src/**']\n---\n",
    );
    store.write("src/main.rs", "fn main() {}\n");
    store.write(
        "stone/milestones/M-1.md",
        "---\ntype: milestone\ntitle: First\nstatus: planned\n---\n",
    );
    let task = |title: &str, status: &str, depends_on: &str| {
        format!("---\ntype: task\ntitle: {title}\nstatus: {status}\nmilestone: M-1\ndepends_on: [{depends_on}]\n---\n")
    };
    store.write("stone/tasks/CS-1.md", &task("Done", "done", ""));
    store.write("stone/tasks/CS-2.md", &task("Old", "archived", ""));
    store.write("stone/tasks/CS-3.md", &task("Check", "review", ""));
    store.write("stone/tasks/CS-4.md", &task("Free", "todo", ""));
    store.write("stone/tasks/CS-5.md", &task("Blocked", "todo", "CS-4"));

    let repo = store.repo();
    let map = repo.project_map().unwrap();
    assert_eq!(map.project, "t");
    assert_eq!(map.git, None);
    let ids = |rows: &[carvestone_engine::project_map::MapTask]| -> Vec<String> {
        rows.iter().map(|row| row.id.clone()).collect()
    };
    assert_eq!(ids(&map.review), ["CS-3"]);
    assert_eq!(ids(&map.ready), ["CS-4"]);
    assert_eq!(map.ready_total, 1);
    let milestone = (map.milestones[0].done, map.milestones[0].total);
    assert_eq!(map.milestones.len(), 1);
    assert_eq!(milestone, (1, 4));
    assert_eq!(map.components.len(), 1);
    assert_eq!(map.components[0].code, ["src/**"]);
    assert_eq!(map.components[0].entries, ["src/main.rs#main"]);

    store.git_commit("feat: app", "2026-10-01T09:00:00+00:00");
    store.write("src/main.rs", "fn main() { }\n");
    let map = store.repo().project_map().unwrap();
    let git = map.git.expect("the store is a git repo");
    assert_eq!(git.changed, 1, "{git:?}");
}

#[test]
fn fill_closed_writes_the_day_once() {
    let store = Store::new();
    let done = "---\ntype: task\ntitle: Ship\nstatus: done\n---\n";
    let archived = "---\ntype: task\ntitle: Old\nstatus: archived\n---\n";
    store.write("stone/tasks/CS-1.md", done);
    store.write("stone/tasks/CS-2.md", archived);
    store.git_commit("chore: scaffold", "2026-09-01T09:00:00+00:00");
    // The newest commit that names the task gives the day, not the commit that added its file.
    store.write("src/lib.rs", "pub fn ship() {}\n");
    store.git_commit("fix: ship it (CS-1)", "2026-10-01T09:00:00+00:00");
    let repo = store.repo();

    let first = repo.fill_closed(false).unwrap();
    assert_eq!(first.len(), 1);
    let text = store.read("stone/tasks/CS-1.md");
    assert!(text.contains("\nclosed: 2026-10-01\n"), "{text}");
    assert_eq!(without(&text, "closed:"), done);
    assert_eq!(store.read("stone/tasks/CS-2.md"), archived);

    let second = repo.fill_closed(false).unwrap();
    assert!(second.is_empty(), "{second:?}");
    assert_eq!(store.read("stone/tasks/CS-1.md"), text);
}

#[test]
fn the_ready_check_lists_every_gap() {
    const READY: &str = "---\ntype: task\ntitle: Ship\nstatus: todo\ncomponents: [arch/x]\npaths: [src/util.rs, src/new.rs]\n---\n\n## Acceptance criteria\n\n- [ ] `cargo test` → 0 failed\n\n## Plan\n\n1. Change `src/util.rs#helper`.\n2. Add `src/new.rs` (new) with `src/new.rs#fresh`.\n";
    const COMPONENTS: &str = "components is empty; set it with `stone edit CS-1 --set components=<arch id>`";
    const PATHS: &str = "paths is empty; set it with `stone edit CS-1 --set paths=<file>,<file>`";
    const NEW_PATH: &str =
        "path src/new.rs does not exist; fix it in paths, or write it in the plan as a code span followed by (new)";
    const NO_CRITERION: &str = "no acceptance criterion; add `- [ ]` lines under ## Acceptance criteria";
    const NO_COMMAND: &str = "criterion 1 carries no command; put the command in a code span";
    const NO_STEPS: &str = "the plan has no numbered steps; write them under ## Plan";
    // (what the row shows, the task file, the gaps it must report)
    let rows: [(&str, String, Vec<&str>); 8] = [
        ("a ready task", READY.to_string(), vec![]),
        (
            "no components",
            READY.replace("components: [arch/x]\n", ""),
            vec![COMPONENTS],
        ),
        (
            "empty paths",
            READY.replace("paths: [src/util.rs, src/new.rs]", "paths: []"),
            vec![PATHS],
        ),
        (
            "a new file without the mark",
            READY.replace(" (new)", ""),
            vec![
                NEW_PATH,
                "plan line 16: src/new.rs#fresh names a file the code graph does not hold; fix the path or mark it (new)",
            ],
        ),
        (
            "a symbol that is gone",
            READY.replace("#helper", "#gone"),
            vec![
                "plan line 15: src/util.rs#gone names a symbol that is not in that file; fix the name or mark it (new)",
            ],
        ),
        (
            "a criterion without a command",
            READY.replace("`cargo test`", "cargo test"),
            vec![NO_COMMAND],
        ),
        (
            "no checkbox",
            READY.replace("- [ ] `cargo test` → 0 failed\n", ""),
            vec![NO_CRITERION],
        ),
        (
            "bullets instead of numbered steps",
            READY.replace("1. ", "- ").replace("2. ", "- "),
            vec![NO_STEPS],
        ),
    ];
    for (what, file, gaps) in rows {
        let store = Store::new();
        store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
        store.write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
        store.write("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n");
        store.write("stone/tasks/CS-1.md", &file);
        let repo = store.repo();

        let found = repo.task_check("CS-1").unwrap();
        assert_eq!(found.id, "CS-1", "{what}");
        assert_eq!(found.gaps, gaps, "{what}");
    }

    // A task without gaps starts, and a second move changes nothing.
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
    store.write("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n");
    store.write("stone/tasks/CS-1.md", READY);
    let repo = store.repo();
    repo.move_task("CS-1", "in-progress", false, false).unwrap();
    assert_eq!(
        store.read("stone/tasks/CS-1.md"),
        READY.replace("status: todo", "status: in-progress")
    );
    assert!(!repo.move_task("CS-1", "in-progress", false, false).unwrap().changed);
}

#[test]
fn errors() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", &TASK.replace("It works.", ""));
    store.write(
        "stone/decisions/ADR-1.md",
        "---\ntype: decision\ntitle: Pick\nstatus: accepted\n---\n",
    );
    store.write(
        "stone/plans/cutover.md",
        "---\ntype: plan\ntitle: Cutover\nstatus: active\n---\n",
    );
    let repo = store.repo();
    let before = store.read("stone/tasks/CS-1.md");
    let cases: [(&str, EngineError, &str); 6] = [
        (
            "unready to in-progress",
            repo.move_task("CS-1", "in-progress", false, false).unwrap_err(),
            "CS-1 is not ready for in-progress: components is empty",
        ),
        (
            "unknown status",
            repo.move_task("CS-1", "bogus", false, false).unwrap_err(),
            "status `bogus` is not valid for a task; use one of: draft, todo, in-progress, review, done, archived",
        ),
        (
            "unknown task",
            repo.move_task("CS-99", "done", false, false).unwrap_err(),
            "CS-99 not found",
        ),
        (
            "done without an outcome",
            repo.move_task("CS-1", "done", false, false).unwrap_err(),
            "CS-1 has no ## Outcome; add one with `stone edit CS-1 --section outcome`",
        ),
        (
            "not a task",
            repo.move_task("ADR-1", "done", false, false).unwrap_err(),
            "ADR-1 is not a task",
        ),
        (
            "a plan",
            repo.move_task("plans/cutover", "done", false, false).unwrap_err(),
            "plans/cutover is a plan, and plans carry no status",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
    assert_eq!(store.read("stone/tasks/CS-1.md"), before);
}
