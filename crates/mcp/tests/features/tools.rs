//! The read tools over the real protocol: handshake, one answer per tool, and replies to bad calls.

use serde_json::{Value, json};

use crate::support::{Client, Store};

const GUIDE: &str = "---\ntype: doc\ntitle: Guide\n---\n\nIntro text.\n\n## Install\n\nRun the installer with sudo.\n\n## Usage\n\nCall the tool daily.\n";
const CARGO: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

fn task(title: &str, status: &str, depends_on: &str) -> String {
    format!("---\ntype: task\ntitle: {title}\nstatus: {status}\n{depends_on}---\n")
}

/// The `id` of every row of a list answer.
fn ids(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn initialize_names_the_server_and_lists_the_four_read_tools() {
    let store = Store::new();
    let mut client = Client::start(store.repo());

    let info = client.initialize().await;
    assert_eq!(info["serverInfo"]["name"], "stone");
    assert_eq!(info["capabilities"], json!({"tools": {}}));
    assert!(
        info["instructions"]
            .as_str()
            .unwrap()
            .starts_with("Read-only view of the stone store")
    );

    let listed = client.request("tools/list", json!({})).await;
    let tools = listed["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["search", "get", "list", "code"]);
    let required: Vec<Value> = tools.iter().map(|t| t["inputSchema"]["required"].clone()).collect();
    assert_eq!(
        required,
        [json!(["query"]), json!(["id"]), Value::Null, json!(["op", "target"])]
    );
}

#[tokio::test]
async fn search_answers_with_hits_that_carry_anchor_and_lines() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    let mut client = Client::connect(store.repo()).await;

    let reply = client.call("search", json!({"query": "installer"})).await;
    assert!(!reply.failed, "{}", reply.text);
    assert_eq!(
        reply.json(),
        json!({
            "query": "installer",
            "hits": [{
                "id": "docs/guide",
                "anchor": "install",
                "path": "stone/docs/guide.md",
                "lines": [8, 11],
                "kind": "doc",
                "snippet": "Run the installer with sudo.",
            }],
            "truncated": false,
        })
    );

    let none = client.call("search", json!({"query": "missing", "limit": 1})).await;
    assert_eq!(none.json(), json!({"query": "missing", "hits": [], "truncated": false}));
}

#[tokio::test]
async fn get_answers_with_an_item_its_backlinks_or_one_section() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    store.write("stone/tasks/CS-1.md", &task("First", "done", ""));
    store.write("stone/tasks/CS-2.md", &task("Second", "todo", "depends_on: [CS-1]\n"));
    let mut client = Client::connect(store.repo()).await;

    let item = client.call("get", json!({"id": "CS-1"})).await.json();
    assert_eq!(item["id"], "tasks/CS-1");
    assert_eq!(
        item["fields"],
        json!({"type": "task", "title": "First", "status": "done"})
    );
    assert_eq!(
        item["backlinks"],
        json!([{
            "id": "tasks/CS-2",
            "anchor": null,
            "path": "stone/tasks/CS-2.md",
            "line": 5,
            "kind": "depends_on",
        }])
    );

    let part = client
        .call("get", json!({"id": "docs/guide", "section": "install"}))
        .await
        .json();
    assert_eq!(part["anchor"], "install");
    assert_eq!(part["text"], "## Install\n\nRun the installer with sudo.\n\n");
    assert_eq!(
        (part["start_line"].clone(), part["end_line"].clone()),
        (json!(8), json!(11))
    );
}

#[tokio::test]
async fn list_filters_by_field_and_ready_gives_the_tasks_whose_dependencies_are_done() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    store.write("stone/tasks/CS-1.md", &task("First", "done", ""));
    store.write("stone/tasks/CS-2.md", &task("Second", "todo", "depends_on: [CS-1]\n"));
    store.write("stone/tasks/CS-3.md", &task("Third", "todo", "depends_on: [CS-2]\n"));
    let mut client = Client::connect(store.repo()).await;

    let todo = client
        .call("list", json!({"type": "task", "status": "todo"}))
        .await
        .json();
    assert_eq!(ids(&todo), ["tasks/CS-2", "tasks/CS-3"]);
    assert_eq!(todo[0]["title"], "Second");
    assert_eq!(todo[0]["depends_on"], json!(["CS-1"]));

    let docs = client.call("list", json!({"type": "doc"})).await.json();
    assert_eq!(ids(&docs), ["docs/guide"]);

    // CS-3 waits for CS-2, which is not done.
    let ready = client.call("list", json!({"ready": true})).await.json();
    assert_eq!(ids(&ready), ["tasks/CS-2"]);
}

#[tokio::test]
async fn code_finds_a_symbol_and_the_places_that_use_it() {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("Cargo.toml", CARGO);
    store.write("src/lib.rs", "pub mod app;\npub mod util;\n");
    store.write("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n");
    store.write(
        "src/app.rs",
        "use crate::util::helper;\n\npub fn run() -> u32 {\n    helper()\n}\n",
    );
    let mut client = Client::connect(store.repo()).await;

    let found = client.call("code", json!({"op": "find", "target": "helper"})).await;
    assert!(!found.failed, "{}", found.text);
    assert_eq!(
        found.json(),
        json!({
            "query": "helper",
            "hits": [{
                "id": "util::helper",
                "anchor": null,
                "path": "src/util.rs",
                "lines": [1, 3],
                "kind": "function",
                "snippet": "pub fn helper() -> u32",
                "exactness": "exact",
            }],
            "truncated": false,
        })
    );

    let used = client
        .call("code", json!({"op": "uses", "target": "helper"}))
        .await
        .json();
    assert_eq!(used["query"], "util::helper");
    let callers: Vec<(&str, &str)> = used["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["id"].as_str().unwrap(), h["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(callers, [("src/app.rs", "imports"), ("app::run", "calls")]);
}

#[tokio::test]
async fn an_unknown_tool_gets_an_error_reply_and_the_server_keeps_answering() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    let mut client = Client::connect(store.repo()).await;

    let reply = client.call("write", json!({})).await;
    assert!(reply.failed);
    assert_eq!(reply.text, "unknown tool `write`; call `tools/list` for the names");

    let next = client.call("search", json!({"query": "installer"})).await;
    assert!(!next.failed, "{}", next.text);
    assert_eq!(next.json()["hits"][0]["id"], "docs/guide");
}

#[tokio::test]
async fn errors() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    store.write("stone/tasks/CS-1.md", &task("First", "done", ""));
    let mut client = Client::connect(store.repo()).await;
    let cases = [
        (
            "search without query",
            "search",
            json!({}),
            "invalid arguments for search: missing field `query`",
        ),
        (
            "search with a number as query",
            "search",
            json!({"query": 5}),
            "invalid arguments for search: invalid type: integer `5`, expected a string",
        ),
        (
            "search with no words",
            "search",
            json!({"query": "?!"}),
            "query has no words; give at least one search term",
        ),
        ("get of an unknown id", "get", json!({"id": "CS-99"}), "CS-99 not found"),
        (
            "get with anchor and section",
            "get",
            json!({"id": "docs/guide#install", "section": "install"}),
            "pass either `id#anchor` or `section`, not both",
        ),
        (
            "list of an unknown type",
            "list",
            json!({"type": "nope"}),
            "unknown type `nope`; one of doc, plan, task",
        ),
        (
            "ready with a status",
            "list",
            json!({"ready": true, "status": "todo"}),
            "`ready` cannot be combined with status, milestone or component",
        ),
        (
            "ready for docs",
            "list",
            json!({"ready": true, "type": "doc"}),
            "`ready` applies to tasks; set type to task or leave it out",
        ),
        (
            "code with an unknown op",
            "code",
            json!({"op": "nope", "target": "helper"}),
            "unknown op `nope`; use find, uses, deps, impact, path or explain",
        ),
        (
            "path without its other end",
            "code",
            json!({"op": "path", "target": "helper"}),
            "path needs `to`",
        ),
        (
            "code without target",
            "code",
            json!({"op": "find"}),
            "invalid arguments for code: missing field `target`",
        ),
    ];
    for (what, tool, args, want) in cases {
        let reply = client.call(tool, args).await;
        assert!(reply.failed, "{what}: answered {}", reply.text);
        assert!(reply.text.contains(want), "{what}: {}", reply.text);
    }
}
