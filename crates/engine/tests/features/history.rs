//! History: `why`, `precedents`, `timeline`, `history` and `commit-msg`, over one small shop
//! whose git history ties commits to tasks.

use carvestone_engine::EngineError;
use carvestone_engine::commit_msg::trailers_for;
use carvestone_engine::history::{History, HistoryEntry, HistoryOpts, history};
use carvestone_engine::precedents::{Lookup, Precedent, Precedents, precedents};
use carvestone_engine::timeline::{Date, EventKind, TimelineOpts, parse_date, timeline};
use carvestone_engine::why::{CommitRef, Related, Why, why};

use crate::support::Store;

const SHOP: &str = "---\ntype: system\ntitle: Shop\n---\n";
const API: &str = "---\ntype: container\ntitle: Api\nparent: arch/shop\ncode: [src/orders.rs, src/payments.rs, src/refunds.rs, src/retries.rs]\n---\n";
const STORE: &str = "---\ntype: container\ntitle: Store\nparent: arch/shop\ncode: [src/store.rs]\n---\n";

fn task(title: &str, status: &str, component: &str, more: &str) -> String {
    format!("---\ntype: task\ntitle: {title}\nstatus: {status}\ncomponents: [{component}]\n{more}---\n")
}

fn decision(status: &str) -> String {
    format!("---\ntype: decision\ntitle: Flat fees\nstatus: {status}\n---\n")
}

/// A shop with two containers and nine commits, oldest first. Each feature commit names its
/// task in a `Refs` trailer, `fix: charge a flat fee` names none, and `fix: round order totals`
/// names CS-1 only in its subject. CS-3 is the one task in progress. The shas come back in order.
fn shop() -> (Store, Vec<String>) {
    let store = Store::new();
    let mut shas = Vec::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("stone/arch/shop.md", SHOP);
    store.write("stone/arch/shop/api.md", API);
    store.write("stone/arch/shop/store.md", STORE);
    store.write("stone/decisions/ADR-1.md", &decision("proposed"));
    store.write("Cargo.toml", "[package]\nname = \"shop\"\nversion = \"0.1.0\"\n");
    store.write(
        "src/lib.rs",
        "pub mod orders;\npub mod payments;\npub mod refunds;\npub mod retries;\npub mod store;\n",
    );
    shas.push(store.git_commit("chore: scaffold the shop", "2026-02-20T09:00:00+00:00"));

    store.write("src/orders.rs", "pub fn total(qty: u32) -> u32 {\n    qty * 10\n}\n");
    store.write(
        "stone/tasks/CS-1.md",
        &task("Add orders endpoint", "in-progress", "arch/shop/api", ""),
    );
    shas.push(store.git_commit(
        "feat: add orders endpoint\n\nRefs: CS-1\nDecision: ADR-1",
        "2026-03-01T09:00:00+00:00",
    ));

    store.write("src/payments.rs", "pub fn pay(amount: u32) -> u32 {\n    amount\n}\n");
    store.write(
        "stone/tasks/CS-2.md",
        &task("Add payments endpoint", "in-progress", "arch/shop/api", ""),
    );
    shas.push(store.git_commit("feat: add payments endpoint\n\nRefs: CS-2", "2026-03-05T09:00:00+00:00"));

    store.write("src/store.rs", "pub fn save(id: u32) -> u32 {\n    id\n}\n");
    store.write(
        "stone/tasks/CS-4.md",
        &task("Add order store", "in-progress", "arch/shop/store", ""),
    );
    shas.push(store.git_commit("feat: add the order store\n\nRefs: CS-4", "2026-03-10T09:00:00+00:00"));

    store.write(
        "src/orders.rs",
        "pub fn total(qty: u32) -> u32 {\n    qty * 10 + 5\n}\n",
    );
    shas.push(store.git_commit("fix: charge a flat fee", "2026-03-15T09:00:00+00:00"));

    store.write("src/refunds.rs", "pub fn refund(amount: u32) -> u32 {\n    amount\n}\n");
    store.write(
        "stone/plans/refunds.md",
        "---\ntype: plan\ntitle: Refunds\nstatus: active\n---\n",
    );
    store.write(
        "stone/tasks/CS-3.md",
        &task(
            "Add refunds endpoint",
            "in-progress",
            "arch/shop/api",
            "links: [ADR-1]\nplan: plans/refunds\n",
        ),
    );
    shas.push(store.git_commit("feat: add refunds endpoint\n\nRefs: CS-3", "2026-03-20T09:00:00+00:00"));

    for (id, title) in [("CS-1", "Add orders endpoint"), ("CS-2", "Add payments endpoint")] {
        store.write(
            &format!("stone/tasks/{id}.md"),
            &task(title, "done", "arch/shop/api", ""),
        );
    }
    store.write(
        "stone/tasks/CS-4.md",
        &task("Add order store", "done", "arch/shop/store", ""),
    );
    store.write("stone/decisions/ADR-1.md", &decision("accepted"));
    shas.push(store.git_commit("docs: close the first tasks", "2026-03-25T09:00:00+00:00"));

    store.write(
        "src/orders.rs",
        "pub fn total(qty: u32) -> u32 {\n    qty * 10 + 5\n}\npub const MAX: u32 = 99;\n",
    );
    shas.push(store.git_commit("fix: round order totals (CS-1)", "2026-04-01T09:00:00+00:00"));

    store.write("src/retries.rs", "pub fn retry(id: u32) -> u32 {\n    id\n}\n");
    store.write(
        "stone/tasks/CS-5.md",
        &task("Add retries endpoint", "review", "arch/shop/api", ""),
    );
    shas.push(store.git_commit("wip: retry refunds\n\nRefs: CS-5", "2026-04-05T09:00:00+00:00"));
    (store, shas)
}

fn related(id: &str, status: &str, title: &str, via: &str, commits: u32) -> Related {
    Related {
        id: id.into(),
        status: Some(status.into()),
        title: Some(title.into()),
        via: via.into(),
        commits,
    }
}

fn commit_ref(sha: &str, date: &str, subject: &str) -> CommitRef {
    CommitRef {
        sha: sha.into(),
        date: date.into(),
        author: "dev".into(),
        subject: subject.into(),
        uncommitted: false,
    }
}

#[test]
fn why_names_the_task_and_commit_behind_a_line() {
    let (store, shas) = shop();
    let repo = store.repo();

    // Line 1 is still the one the orders commit wrote.
    let found = why(&repo, "src/orders.rs", Some(1), None).unwrap();
    assert_eq!(
        found,
        Why {
            path: "src/orders.rs".into(),
            line: Some(1),
            component: Some("arch/shop/api".into()),
            commits: vec![commit_ref(
                &shas[1],
                "2026-03-01T09:00:00Z",
                "feat: add orders endpoint"
            )],
            tasks: vec![related("CS-1", "done", "Add orders endpoint", "trailer", 1)],
            plans: vec![],
            decisions: vec![related("ADR-1", "accepted", "Flat fees", "trailer", 1)],
            docs: vec![],
            fallback: None,
            shallow: false,
            truncated: false,
            omitted: 0,
        }
    );

    // Line 2 was rewritten by a commit that names no task, so the open tasks of the component
    // answer, and the plan and decision of the task come along.
    let found = why(&repo, "src/orders.rs", Some(2), None).unwrap();
    assert_eq!(
        found.commits,
        [commit_ref(&shas[4], "2026-03-15T09:00:00Z", "fix: charge a flat fee")]
    );
    assert_eq!(found.fallback.as_deref(), Some("component"));
    assert_eq!(
        found.tasks,
        [
            related("CS-3", "in-progress", "Add refunds endpoint", "component", 0),
            related("CS-5", "review", "Add retries endpoint", "component", 0),
        ]
    );
    assert_eq!(found.plans, [related("plans/refunds", "active", "Refunds", "task", 0)]);
    assert_eq!(found.decisions, [related("ADR-1", "accepted", "Flat fees", "task", 0)]);
}

#[test]
fn precedents_find_finished_tasks_that_made_the_same_change() {
    let (store, _) = shop();
    let repo = store.repo();

    // CS-3 added one file with a function to the api. CS-1 and CS-2 did the same there; CS-4 added
    // such a file to the other container; CS-5 is not finished, so it never shows.
    let found = precedents(
        &repo,
        &Lookup {
            task: Some("CS-3".into()),
            ..Lookup::default()
        },
    )
    .unwrap();
    let in_api = ["component arch/shop/api", "added function", "folder src"];
    assert_eq!(
        found,
        Precedents {
            basis: "the commits of CS-3".into(),
            fallback: None,
            hits: vec![
                Precedent {
                    id: "CS-2".into(),
                    title: Some("Add payments endpoint".into()),
                    score: 6,
                    matched: in_api.map(String::from).to_vec(),
                    files: vec!["src/payments.rs".into()],
                    more_files: 0,
                },
                Precedent {
                    id: "CS-1".into(),
                    title: Some("Add orders endpoint".into()),
                    score: 6,
                    matched: in_api.map(String::from).to_vec(),
                    files: vec!["src/orders.rs".into()],
                    more_files: 0,
                },
                Precedent {
                    id: "CS-4".into(),
                    title: Some("Add order store".into()),
                    score: 3,
                    matched: vec!["added function".into(), "folder src".into()],
                    files: vec!["src/store.rs".into()],
                    more_files: 0,
                },
            ],
            truncated: false,
            omitted: 0,
        }
    );
}

#[test]
fn timeline_lists_commits_and_closed_work_newest_first() {
    let (store, _) = shop();
    let repo = store.repo();

    let found = timeline(&repo, &TimelineOpts::default()).unwrap();
    let got: Vec<(&str, &str, &str)> = found
        .iter()
        .map(|e| (&e.at[..10], e.kind.name(), e.title.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("2026-04-05", "commit", "wip: retry refunds"),
            ("2026-04-01", "commit", "fix: round order totals (CS-1)"),
            ("2026-03-25", "commit", "docs: close the first tasks"),
            ("2026-03-25", "task_done", "Add orders endpoint"),
            ("2026-03-25", "task_done", "Add payments endpoint"),
            ("2026-03-25", "task_done", "Add order store"),
            ("2026-03-25", "decision", "Flat fees"),
            ("2026-03-20", "commit", "feat: add refunds endpoint"),
            ("2026-03-15", "commit", "fix: charge a flat fee"),
            ("2026-03-10", "commit", "feat: add the order store"),
            ("2026-03-05", "commit", "feat: add payments endpoint"),
            ("2026-03-01", "commit", "feat: add orders endpoint"),
            ("2026-02-20", "commit", "chore: scaffold the shop"),
        ]
    );
    let items: Vec<&str> = found
        .iter()
        .filter(|e| e.kind != EventKind::Commit)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(items, ["CS-1", "CS-2", "CS-4", "ADR-1"]);

    let newest = timeline(
        &repo,
        &TimelineOpts {
            limit: 2,
            ..TimelineOpts::default()
        },
    )
    .unwrap();
    let subjects: Vec<&str> = newest.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(subjects, ["wip: retry refunds", "fix: round order totals (CS-1)"]);
}

#[test]
fn history_lists_the_commits_of_an_item() {
    let (store, shas) = shop();
    let repo = store.repo();

    // A task lists the commits of its file, and the commits tied to it by trailer or by id.
    let found = history(&repo, "CS-1", &HistoryOpts::default()).unwrap();
    assert_eq!(
        found,
        History {
            id: "CS-1".into(),
            entries: vec![
                HistoryEntry {
                    sha: shas[7].clone(),
                    date: "2026-04-01T09:00:00Z".into(),
                    author: "dev".into(),
                    subject: "fix: round order totals (CS-1)".into(),
                    refs: vec![],
                    diff: None,
                },
                HistoryEntry {
                    sha: shas[6].clone(),
                    date: "2026-03-25T09:00:00Z".into(),
                    author: "dev".into(),
                    subject: "docs: close the first tasks".into(),
                    refs: vec![],
                    diff: Some(
                        "@@ -1,6 +1,6 @@\n ---\n type: task\n title: Add orders endpoint\n-status: in-progress\n+status: done\n components: [arch/shop/api]\n ---\n"
                            .into()
                    ),
                },
                HistoryEntry {
                    sha: shas[1].clone(),
                    date: "2026-03-01T09:00:00Z".into(),
                    author: "dev".into(),
                    subject: "feat: add orders endpoint".into(),
                    refs: vec!["CS-1".into()],
                    diff: Some(
                        "@@ -0,0 +1,6 @@\n+---\n+type: task\n+title: Add orders endpoint\n+status: in-progress\n+components: [arch/shop/api]\n+---\n"
                            .into()
                    ),
                },
            ],
            older: 0,
            filtered: false,
        }
    );

    // Any other item lists only the commits of its own file.
    let found = history(&repo, "ADR-1", &HistoryOpts::default()).unwrap();
    let subjects: Vec<&str> = found.entries.iter().map(|e| e.subject.as_str()).collect();
    assert_eq!(subjects, ["docs: close the first tasks", "chore: scaffold the shop"]);
}

#[test]
fn commit_msg_gives_the_trailers_of_the_task_in_progress() {
    let (store, _) = shop();
    let repo = store.repo();

    assert_eq!(
        trailers_for(&repo, None).unwrap(),
        ["Refs: CS-3", "Plan: plans/refunds", "Decision: ADR-1"]
    );
    assert_eq!(trailers_for(&repo, Some("CS-1")).unwrap(), ["Refs: CS-1"]);
}

#[test]
fn errors() {
    let (store, _) = shop();
    store.write("src/draft.rs", "pub fn draft() {}\n");
    let repo = store.repo();
    let from_task = |task: &str| Lookup {
        task: Some(task.into()),
        ..Lookup::default()
    };
    let mut cases: Vec<(&str, EngineError, &str)> = vec![
        (
            "untracked file",
            why(&repo, "src/draft.rs", None, None).unwrap_err(),
            "src/draft.rs is not tracked by git; run `git add`",
        ),
        (
            "line zero",
            why(&repo, "src/orders.rs", Some(0), None).unwrap_err(),
            "lines start at 1",
        ),
        (
            "line past the end",
            why(&repo, "src/orders.rs", Some(9), None).unwrap_err(),
            "src/orders.rs has 4 lines",
        ),
        (
            "nothing to look up",
            precedents(&repo, &Lookup::default()).unwrap_err(),
            "give a task, --component or --role",
        ),
        (
            "not a task",
            precedents(&repo, &from_task("ADR-1")).unwrap_err(),
            "ADR-1 is not a task; give a task id, for example CS-12",
        ),
        (
            "bad date",
            parse_date("--since", "yesterday", Date::today()).unwrap_err(),
            "--since must be YYYY-MM-DD or Nd",
        ),
        (
            "bad kind",
            EventKind::parse("bogus").unwrap_err(),
            "unknown kind `bogus`; one of commit, task_done, decision, doc",
        ),
        (
            "unknown item",
            history(&repo, "CS-99", &HistoryOpts::default()).unwrap_err(),
            "CS-99 not found",
        ),
        (
            "unknown section",
            history(&repo, "CS-1#nothing", &HistoryOpts::default()).unwrap_err(),
            "CS-1 has no section #nothing",
        ),
        (
            "trailers of a decision",
            trailers_for(&repo, Some("ADR-1")).unwrap_err(),
            "ADR-1 is not a task",
        ),
    ];

    store.write(
        "stone/tasks/CS-3.md",
        &task("Add refunds endpoint", "review", "arch/shop/api", ""),
    );
    cases.push((
        "no task in progress",
        trailers_for(&repo, None).unwrap_err(),
        "no task is in progress; pass --task CS-n",
    ));
    store.write(
        "stone/tasks/CS-3.md",
        &task("Add refunds endpoint", "in-progress", "arch/shop/api", ""),
    );
    store.write(
        "stone/tasks/CS-5.md",
        &task("Add retries endpoint", "in-progress", "arch/shop/api", ""),
    );
    cases.push((
        "two tasks in progress",
        trailers_for(&repo, None).unwrap_err(),
        "2 tasks are in progress (CS-3, CS-5); pass --task",
    ));
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
}
