//! Product workspace: one stone over a product whose code lives in several git repos.

use carvestone_engine::EngineError;
use carvestone_engine::code::{CodeQuery, find, outline, uses};
use carvestone_engine::code_graph::impact;
use carvestone_engine::code_native::{NativeQuery, native};
use carvestone_engine::create::init;
use carvestone_engine::product::feature;
use carvestone_engine::search::Hit;
use carvestone_engine::session_hook;
use carvestone_engine::why::why;

use crate::support::Store;

fn query(target: &str) -> CodeQuery {
    CodeQuery {
        target: target.into(),
        ..CodeQuery::default()
    }
}

fn lines(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(Hit::line).collect()
}

/// A product root with no git of its own and the files of three repos: `stone/`, `api/` and
/// `portal/`, none committed yet. The portal crate depends on the api crate and calls
/// `api::orders::total` from `checkout`.
fn files() -> Store {
    let store = Store::new();
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots: [api/, portal/]\n",
    );
    store.write("api/Cargo.toml", "[package]\nname = \"api\"\nversion = \"0.1.0\"\n");
    store.write("api/src/lib.rs", "pub mod orders;\n");
    store.write(
        "api/src/orders.rs",
        "pub fn total(qty: u32) -> u32 {\n    qty * 10\n}\n",
    );
    store.write(
        "portal/Cargo.toml",
        "[package]\nname = \"portal\"\nversion = \"0.1.0\"\n\n[dependencies]\napi = { path = \"../api\" }\n",
    );
    store.write("portal/src/lib.rs", "pub mod cart;\n");
    store.write(
        "portal/src/cart.rs",
        "use api::orders::total;\n\npub fn checkout() -> u32 {\n    total(2)\n}\n",
    );
    store
}

fn commit(store: &Store, dirs: &[&str]) {
    for dir in dirs {
        store.git_commit_in(dir, "feat: start", "2026-03-01T09:00:00+00:00");
    }
}

/// The same product with each folder its own git repo.
fn product() -> Store {
    let store = files();
    commit(&store, &["stone", "api", "portal"]);
    store
}

#[test]
fn code_in_every_repo_is_found_from_the_product_root() {
    let store = product();
    let repo = store.repo();

    assert_eq!(
        lines(&find(&repo, &query("total")).unwrap().hits),
        ["orders::total  api/src/orders.rs:L1-3  function  exact  pub fn total(qty: u32) -> u32"]
    );
    assert_eq!(
        lines(&find(&repo, &query("checkout")).unwrap().hits),
        ["cart::checkout  portal/src/cart.rs:L3-5  function  exact  pub fn checkout() -> u32"]
    );
    assert_eq!(
        outline(&repo, &query("portal/src/cart.rs")).unwrap().path,
        "portal/src/cart.rs"
    );
}

#[test]
fn a_use_crosses_from_one_repo_into_the_other() {
    let store = product();
    let repo = store.repo();

    assert_eq!(
        lines(&uses(&repo, &query("total")).unwrap().hits),
        [
            "portal/src/cart.rs  portal/src/cart.rs:L1-5  imports  exact  @L1",
            "cart::checkout  portal/src/cart.rs:L3-5  calls  exact  @L4 pub fn checkout() -> u32",
        ]
    );
    assert_eq!(
        lines(&impact(&repo, &query("total")).unwrap().dependents),
        [
            "portal/src/cart.rs  portal/src/cart.rs:L1-5  file  exact  depth 1",
            "cart::checkout  portal/src/cart.rs:L3-5  function  exact  depth 1 pub fn checkout() -> u32",
        ]
    );
}

#[test]
fn a_repo_git_cannot_read_is_named_and_the_other_still_answers() {
    let store = files();
    commit(&store, &["stone", "api"]);
    // A `.git` file that points nowhere, as a broken worktree or submodule leaves behind.
    store.write("portal/.git", "gitdir: elsewhere\n");
    let repo = store.repo();

    let problems: Vec<String> = repo
        .check()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .filter(|problem| problem.contains(": git: "))
        .collect();
    assert_eq!(
        problems,
        [
            "stone/config.yml:1: git: history of portal is missing: git is not available in this folder; fix git in that folder and the next refresh picks it up"
        ]
    );

    let found = why(&repo, "api/src/orders.rs", Some(1), None).unwrap();
    assert_eq!(found.commits.len(), 1);
    assert_eq!(found.commits[0].subject, "feat: start");
    assert_eq!(
        lines(&find(&repo, &query("total")).unwrap().hits),
        ["orders::total  api/src/orders.rs:L1-3  function  exact  pub fn total(qty: u32) -> u32"]
    );
    assert_eq!(
        lines(&find(&repo, &query("checkout")).unwrap().hits),
        ["cart::checkout  portal/src/cart.rs:L3-5  function  exact  pub fn checkout() -> u32"]
    );
}

#[test]
fn init_inside_a_repo_indexes_the_whole_repo() {
    let store = Store::new();
    store.write("README.md", "# demo\n");
    store.write(".gitignore", "target/\n");
    store.git_commit("feat: start", "2026-03-01T09:00:00+00:00");
    std::fs::remove_file(store.path("stone/config.yml")).unwrap();
    // Untracked on purpose: a new project has files git does not know yet.
    store.write("game/Player.cs", "public class Player\n{\n}\n");

    let done = init(&store.path(""), Some("CS")).unwrap();

    assert_eq!(done.code_roots, ["."]);
    assert!(
        store
            .read("stone/config.yml")
            .ends_with("task_prefix: CS\ncode_roots:\n  - '.'\n")
    );
    let repo = store.repo();
    assert_eq!(
        lines(&find(&repo, &query("Player")).unwrap().hits),
        ["Player  game/Player.cs:L1-3  type  exact  public class Player"]
    );
    let owner_problems = |store: &Store| -> Vec<String> {
        store
            .repo()
            .check()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .filter(|problem| problem.contains(": owner: "))
            .collect()
    };
    let none = Vec::<String>::new();
    assert_eq!(owner_problems(&store), none);

    // The first component claims `game/`; the README and `.gitignore` need no owner.
    store.write(
        "stone/arch/game.md",
        "---\ntype: component\ntitle: Game\ncode: [game/**]\n---\n",
    );
    store.git_commit("feat: game", "2026-03-02T09:00:00+00:00");
    assert_eq!(owner_problems(&store), none);

    // A code file outside it still has no owner.
    store.write("tools/Build.cs", "public class Build\n{\n}\n");
    store.git_commit("feat: tools", "2026-03-03T09:00:00+00:00");
    let stray = owner_problems(&store);
    assert_eq!(stray.len(), 1, "{stray:?}");
    assert!(stray[0].starts_with("tools/Build.cs:1: owner: "), "{stray:?}");
}

#[test]
fn whole_repo_root_links_a_feature_by_a_named_path() {
    let store = Store::new();
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots:\n  - '.'\n",
    );
    store.write(
        "stone/docs/feature-login.md",
        "---\ntype: doc\ntitle: Login\n---\n\nPlayers sign in through `game/Player.cs`, e.g Node.js tools stay out.\n",
    );
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: Player\nstatus: todo\npaths: [game/Player.cs]\n---\n",
    );
    store.write(
        "stone/tasks/CS-2.md",
        "---\ntype: task\ntitle: Words\nstatus: todo\npaths: [e.g, Node.js]\n---\n",
    );
    store.git_commit("feat: start", "2026-03-01T09:00:00+00:00");

    let detail = feature(&store.repo(), "feature-login").unwrap();

    let tasks: Vec<(String, Option<String>)> = detail
        .links
        .tasks
        .iter()
        .map(|task| (task.id.clone(), task.via.clone()))
        .collect();
    assert_eq!(tasks, [("CS-1".to_string(), Some("path game/Player.cs".to_string()))]);
}

#[test]
fn a_feature_row_names_its_services_and_says_when_its_doc_is_stale() {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\n");
    store.write("stone/arch/api.md", "---\ntype: container\ntitle: Api\n---\n");
    store.write(
        "stone/arch/api/orders.md",
        "---\ntype: component\ntitle: Orders\nparent: arch/api\n---\n",
    );
    store.write(
        "stone/docs/feature-login.md",
        "---\ntype: doc\ntitle: Login\nstale_after: 2020-01-01\n---\n\nRuns on [[arch/api]] and [[arch/api/orders]].\n",
    );
    store.write(
        "stone/docs/feature-logout.md",
        "---\ntype: doc\ntitle: Logout\nstale_after: 2999-01-01\n---\n",
    );
    store.git_commit("feat: start", "2026-03-01T09:00:00+00:00");

    let rows = carvestone_engine::product::product(&store.repo()).unwrap().features;

    let seen: Vec<(&str, &[String], bool)> = rows
        .iter()
        .map(|row| (row.title.as_str(), row.services.as_slice(), row.stale))
        .collect();
    assert_eq!(
        seen,
        [("Login", &["Api".to_string()][..], true), ("Logout", &[][..], false)]
    );
}

/// The `hooks` object as `serde_json::to_string_pretty` prints it, without the leading `{`.
const HOOK_TAIL: &str = r#"  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "stone map"
          }
        ]
      }
    ]
  }
}"#;

#[test]
fn the_session_hook_is_added_once_and_keeps_the_settings() {
    let empty = format!("{{\n{HOOK_TAIL}\n");
    assert_eq!(session_hook::apply("").unwrap(), empty);
    assert_eq!(session_hook::apply(" \n\t").unwrap(), empty);

    // User keys and their values stay, in their order, and `hooks` goes last.
    let ordered = format!(
        "{{\n  \"permissions\": {{\n    \"allow\": [\n      \"Bash(ls)\"\n    ]\n  }},\n  \"env\": {{\n    \"A\": \"1\"\n  }},\n{HOOK_TAIL}\n"
    );
    assert_eq!(
        session_hook::apply(r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"A":"1"}}"#).unwrap(),
        ordered
    );

    // A CRLF file stays CRLF, and a second run leaves it byte for byte.
    let crlf = format!(
        "{{\r\n  \"env\": {{\r\n    \"A\": \"1\"\r\n  }},\r\n{}\r\n",
        HOOK_TAIL.replace('\n', "\r\n")
    );
    let crlf_merged = session_hook::apply("{\r\n  \"env\": {\r\n    \"A\": \"1\"\r\n  }\r\n}\r\n").unwrap();
    assert_eq!(crlf_merged, crlf);
    assert_eq!(session_hook::apply(&crlf_merged).unwrap(), crlf_merged);

    // Another entry stays first; the stone one is added after it.
    let other = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#;
    let merged = session_hook::apply(other).unwrap();
    let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
    let entries = value["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(
        entries.as_slice(),
        [
            serde_json::json!({"hooks": [{"type": "command", "command": "echo hi"}]}),
            serde_json::json!({"hooks": [{"type": "command", "command": "stone map"}]}),
        ],
        "{merged}"
    );

    assert_eq!(session_hook::apply(&empty).unwrap(), empty);

    // A repository that runs its own build keeps its command.
    let own = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"target/debug/stone.exe map"}]}]}}"#;
    assert_eq!(session_hook::apply(own).unwrap(), own);

    assert!(session_hook::apply("[1]").is_err());
    assert!(session_hook::apply(r#"{"hooks": 3}"#).is_err());
}

#[test]
fn errors() {
    let store = product();
    let repo = store.repo();
    // An empty search path leaves no indexer to run, so only the repo name is judged.
    let unknown_repo = NativeQuery {
        repo: Some("billing".into()),
        search_path: Some("".into()),
        ..NativeQuery::default()
    };
    let cases: [(&str, EngineError, &str); 2] = [
        (
            "file in no repo",
            why(&repo, "docs/readme.md", None, None).unwrap_err(),
            "docs/readme.md is not under git; run `git init` in its folder or list the folder in code_roots",
        ),
        (
            "unknown repo",
            native(&repo, &unknown_repo).unwrap_err(),
            "no repo billing in this product; use one of: api, stone, portal",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
}
