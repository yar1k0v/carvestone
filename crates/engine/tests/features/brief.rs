//! `stone brief`: one answer from a task to its starting point.

use carvestone_engine::EngineError;
use carvestone_engine::brief::{DEFAULT_BUDGET, brief};
use carvestone_engine::tiers::{Part, Tier, fit};

use crate::support::Store;

const TASK: &str = "---\ntype: task\ntitle: Ship\nstatus: todo\nlinks: [ADR-1]\ncomponents: [arch/api]\npaths: [src/a.rs]\n---\n\n## Description\n\nDo the thing, see [[docs/guide#setup]].\n\n## Acceptance criteria\n\n- [ ] first criterion\n- [ ] second criterion\n\n## Plan\n\n1. first step\n2. second step\n";
const BARE: &str = "---\ntype: task\ntitle: Bare\nstatus: todo\n---\n\n## Description\n\nJust text.\n\n## Acceptance criteria\n\n- [ ] only criterion\n";

fn store() -> Store {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", TASK);
    store.write("stone/tasks/CS-2.md", BARE);
    store.write(
        "stone/decisions/ADR-1.md",
        "---\ntype: decision\ntitle: Pick\nstatus: accepted\n---\n\n## Context\n\nWhy we looked.\n\n## Decision\n\nPick the cheap one.\n",
    );
    let setup: String = (1..=30).map(|n| format!("setup line {n}\n\n")).collect();
    store.write(
        "stone/docs/guide.md",
        &format!("---\ntype: doc\ntitle: Guide\n---\n\n## Setup\n\n{setup}"),
    );
    store.write("stone/arch/api.md", "---\ntype: system\ntitle: Api\n---\n");
    store
}

fn part<'a>(parts: &'a [Part], name: &str) -> &'a Part {
    parts
        .iter()
        .find(|part| part.name == name)
        .unwrap_or_else(|| panic!("no part {name}"))
}

#[test]
fn a_brief_holds_the_task_whole_and_its_links_by_tier() {
    let store = store();
    let repo = store.repo();
    let found = brief(&repo, "CS-1").unwrap();
    assert_eq!(found.task, "CS-1");

    let names: Vec<&str> = found.parts.iter().map(|part| part.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "task",
            "links",
            "ADR-1#decision",
            "docs/guide#setup",
            "entry points",
            "outline src/a.rs"
        ]
    );
    let tiers: Vec<Tier> = found.parts.iter().map(|part| part.tier).collect();
    assert_eq!(
        tiers,
        [
            Tier::Critical,
            Tier::Critical,
            Tier::Optional,
            Tier::Optional,
            Tier::Optional,
            Tier::Optional
        ]
    );

    let task = part(&found.parts, "task");
    let has = |text: &str| task.lines.iter().any(|line| line.contains(text));
    assert!(has("Do the thing"));
    for text in ["first criterion", "second criterion", "first step", "second step"] {
        assert!(has(text), "{text}");
    }
    assert!(task.lines.iter().any(|line| line == "Paths: src/a.rs"));
    assert!(task.lines[0].starts_with("CS-1  Ship  todo"));

    // Links print in file order: relation fields first, then the body.
    let links = part(&found.parts, "links");
    assert_eq!(links.lines.len(), 3);
    for (line, start) in links.lines.iter().zip(["ADR-1  ", "arch/api  ", "docs/guide  "]) {
        assert!(line.starts_with(start) && line.ends_with(" tokens"), "{line}");
    }
    let decision = part(&found.parts, "ADR-1#decision").lines.join("\n");
    assert!(decision.contains("Pick the cheap one."));
    assert!(!decision.contains("Why we looked."));

    let cut = fit(&found.parts, Some(60));
    for line in task.lines.iter().chain(&links.lines) {
        assert!(cut.text.contains(line.as_str()), "{line}");
    }
    assert!(!cut.text.contains("setup line 30"));
    assert!(cut.text.lines().last().unwrap().ends_with("; raise --budget"));

    let whole = fit(&found.parts, Some(DEFAULT_BUDGET));
    assert!(whole.text.contains("setup line 30"));
    assert!(!whole.text.contains("left out"));

    let bare = brief(&repo, "CS-2").unwrap();
    assert!(
        part(&bare.parts, "task")
            .lines
            .iter()
            .any(|line| line == "(no ## Plan section)")
    );
    assert!(bare.parts.iter().all(|part| part.name != "links"));
}

#[test]
fn a_brief_lists_the_plan_targets_with_their_lines() {
    const PLANNED: &str = "---\ntype: task\ntitle: Ship\nstatus: todo\npaths: [src/util.rs]\n---\n\n## Acceptance criteria\n\n- [ ] `cargo test` → 0 failed\n\n## Plan\n\n1. Change `src/util.rs#helper`.\n2. Add `src/new.rs#fresh` (new).\n3. Fix `src/util.rs#gone`.\n4. Call `src/util.rs#helper` again.\n5. Use `src/two.rs#B::new`.\n6. Use `src/two.rs#new`.\n7. Read `README.md#usage`.\n";
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
    store.write("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n");
    store.write(
        "src/two.rs",
        "pub struct A;\npub struct B;\nimpl A {\n    pub fn new() -> A {\n        A\n    }\n}\nimpl B {\n    pub fn new() -> B {\n        B\n    }\n}\n",
    );
    store.write("README.md", "# Demo\n\nUsage notes.\n");
    store.write("stone/tasks/CS-1.md", PLANNED);
    let repo = store.repo();

    let found = brief(&repo, "CS-1").unwrap();
    assert_eq!(found.parts[0].name, "task");
    let targets = &found.parts[1];
    assert_eq!(targets.name, "plan targets");
    assert_eq!(targets.tier, Tier::Critical);
    assert_eq!(
        targets.lines,
        [
            "src/util.rs#helper  L1-3",
            "src/new.rs#fresh  (new)",
            "src/util.rs#gone  not found",
            "src/two.rs#B::new  L9-11",
            "src/two.rs#new  ambiguous, 2 matches: L4-6, L9-11"
        ]
    );

    // The part is never cut, however small the budget.
    let cut = fit(&found.parts, Some(60));
    for line in &targets.lines {
        assert!(cut.text.contains(line.as_str()), "{line}");
    }
}

#[test]
fn a_brief_refuses_what_is_not_a_task() {
    let store = store();
    let repo = store.repo();
    for id in ["ADR-1", "arch/api"] {
        let error = brief(&repo, id).unwrap_err();
        assert!(error.to_string().contains("is not a task"), "{id}: {error}");
    }
    assert!(matches!(
        brief(&repo, "CS-99").unwrap_err(),
        EngineError::NotFound { .. }
    ));
}
