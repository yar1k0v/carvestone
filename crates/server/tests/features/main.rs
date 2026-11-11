//! The web contract over the real router: read routes, an owner write, the event stream, the origin guard,
//! and one table for wrong input.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use carvestone_engine::Repo;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

static NEXT: AtomicU32 = AtomicU32::new(0);

const GUIDE: &str = "---\ntype: doc\ntitle: Guide\n---\n\nIntro text.\n\n## Install\n\nRun the installer with sudo.\n\n## Usage\n\nCall the tool daily.\n";
const CARGO: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

/// A temp store with prefix `CS`, removed on drop.
struct Store(PathBuf);

impl Store {
    fn new() -> Store {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-server-features-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store(dir);
        store.write("stone/config.yml", "project: t\ntask_prefix: CS\n");
        store
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap()
    }

    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).exists()
    }

    fn repo(&self) -> Repo {
        Repo::open(&self.0).unwrap().with_author("test".into())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn task(title: &str, status: &str, depends_on: &str) -> String {
    format!("---\ntype: task\ntitle: {title}\nstatus: {status}\n{depends_on}---\n")
}

/// What a route answered.
struct Answer {
    status: StatusCode,
    etag: Option<String>,
    text: String,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|err| panic!("not JSON ({err}): {}", self.text))
    }
}

/// One request to `app`; the `Host` is `localhost` unless the headers name one.
async fn send(app: &Router, method: Method, uri: &str, headers: &[(&str, &str)], body: Option<&str>) -> Answer {
    let mut req = Request::builder().method(method).uri(uri);
    if !headers.iter().any(|(name, _)| *name == "host") {
        req = req.header(header::HOST, "localhost");
    }
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let req = req
        .body(body.map_or_else(Body::empty, |text| Body::from(text.to_string())))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let etag = response
        .headers()
        .get(header::ETAG)
        .map(|value| value.to_str().unwrap().to_string());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Answer {
        status,
        etag,
        text: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

async fn get(app: &Router, uri: &str) -> Answer {
    send(app, Method::GET, uri, &[], None).await
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
async fn read_routes_answer_with_items_search_hits_and_code() {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("stone/docs/guide.md", GUIDE);
    store.write("stone/tasks/CS-1.md", &task("First", "done", ""));
    store.write("stone/tasks/CS-2.md", &task("Second", "todo", "depends_on: [CS-1]\n"));
    store.write("Cargo.toml", CARGO);
    store.write("src/lib.rs", "pub mod util;\n");
    store.write("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n");
    let app = carvestone_server::router(store.repo());

    let todo = get(&app, "/api/items?type=task&status=todo").await;
    assert_eq!(todo.status, StatusCode::OK);
    let rows = todo.json();
    assert_eq!(ids(&rows), ["tasks/CS-2"]);
    assert_eq!(rows[0]["title"], "Second");
    assert_eq!(rows[0]["depends_on"], json!(["CS-1"]));

    let item = get(&app, "/api/items/CS-1").await;
    assert_eq!(item.status, StatusCode::OK);
    let body = item.json();
    assert_eq!(body["id"], "tasks/CS-1");
    assert_eq!(
        body["fields"],
        json!({"type": "task", "title": "First", "status": "done"})
    );
    assert_eq!(body["backlinks"][0]["id"], "tasks/CS-2");
    assert_eq!(item.etag.unwrap(), format!("\"{}\"", body["etag"].as_str().unwrap()));

    let part = get(&app, "/api/items/docs/guide?section=install").await.json();
    assert_eq!(part["text"], "## Install\n\nRun the installer with sudo.\n\n");

    let found = get(&app, "/api/search?q=installer").await;
    assert_eq!(
        found.json(),
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

    let code = get(&app, "/api/code/find?target=helper").await;
    assert_eq!(code.status, StatusCode::OK);
    let hit = &code.json()["hits"][0];
    assert_eq!(hit["id"], "util::helper");
    assert_eq!(hit["path"], "src/util.rs");
    assert_eq!(hit["lines"], json!([1, 3]));
}

#[tokio::test]
async fn a_patch_and_a_create_save_the_files_the_cli_would_write() {
    let store = Store::new();
    store.write("stone/tasks/CS-2.md", &task("Second", "todo", "depends_on: [CS-1]\n"));
    let app = carvestone_server::router(store.repo());
    let etag = get(&app, "/api/items/CS-2").await.etag.unwrap();

    let patched = send(
        &app,
        Method::PATCH,
        "/api/items/CS-2",
        &[("if-match", &etag)],
        Some(r#"{"set": {"status": "in-progress"}}"#),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK);
    let reply = patched.json();
    assert_eq!((&reply["id"], &reply["changed"]), (&json!("CS-2"), &json!(true)));
    assert_eq!(
        store.read("stone/tasks/CS-2.md"),
        "---\ntype: task\ntitle: Second\nstatus: in-progress\ndepends_on: [CS-1]\n---\n"
    );
    // The reply carries the etag a later write must send.
    let now = get(&app, "/api/items/CS-2").await.etag.unwrap();
    assert_eq!(format!("\"{}\"", reply["etag"].as_str().unwrap()), now);

    // Sending a task back is one write: the status and the comment land in the same file.
    // The web client sends the etag in the body as `if_match`, so the test does the same.
    store.write("stone/tasks/CS-1.md", &task("First", "review", ""));
    let review_etag = get(&app, "/api/items/CS-1").await.etag.unwrap();
    let send_back =
        format!(r#"{{"set": {{"status": "todo"}}, "note": "Sent back: fix the title", "if_match": {review_etag}}}"#);
    let sent_back = send(&app, Method::PATCH, "/api/items/CS-1", &[], Some(&send_back)).await;
    assert_eq!(sent_back.status, StatusCode::OK, "{}", sent_back.text);
    let saved = store.read("stone/tasks/CS-1.md");
    assert!(saved.contains("\nstatus: todo\n"), "{saved}");
    assert!(
        saved.ends_with(&format!(
            "\n## Notes\n\n- {}: Sent back: fix the title\n",
            carvestone_engine::create::today()
        )),
        "{saved}"
    );
    // The same body a second time carries the old etag, so a second save would answer 409.
    let again = send(&app, Method::PATCH, "/api/items/CS-1", &[], Some(&send_back)).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text);
    assert_eq!(store.read("stone/tasks/CS-1.md"), saved);

    let created = send(
        &app,
        Method::POST,
        "/api/items",
        &[],
        Some(r#"{"type": "task", "title": "Third"}"#),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let made = created.json();
    assert_eq!(
        (&made["id"], &made["path"]),
        (&json!("CS-3"), &json!("stone/tasks/CS-3.md"))
    );
    assert!(store.read("stone/tasks/CS-3.md").contains("title: Third\n"));
}

#[tokio::test]
async fn an_edit_made_outside_shows_on_the_event_stream() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", &task("First", "todo", ""));
    let (app, _watcher) = carvestone_server::watched_router(store.repo()).unwrap();
    let request = Request::builder()
        .uri("/api/events")
        .header(header::HOST, "localhost")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/event-stream");
    let mut stream = response.into_body();

    // Another process saves the file; the server did not write it.
    store.write("stone/tasks/CS-1.md", &task("First", "done", ""));

    let heard = tokio::time::timeout(Duration::from_secs(15), async {
        let mut text = String::new();
        while !text.ends_with("\n\n") || !text.contains("event: items") {
            let frame = stream.frame().await.unwrap().unwrap();
            if let Ok(data) = frame.into_data() {
                text.push_str(&String::from_utf8_lossy(&data));
            }
        }
        text
    })
    .await
    .expect("no items event within 15 seconds");
    assert_eq!(heard, "event: items\ndata: {\"ids\":[\"CS-1\",\"tasks/CS-1\"]}\n\n");
}

#[tokio::test]
async fn the_guard_refuses_a_write_from_another_origin_and_a_foreign_host() {
    let store = Store::new();
    store.write("stone/tasks/CS-1.md", &task("First", "todo", ""));
    let app = carvestone_server::router(store.repo());
    let create = r#"{"type": "task", "title": "Second"}"#;

    let cross = send(
        &app,
        Method::POST,
        "/api/items",
        &[("origin", "http://evil.example")],
        Some(create),
    )
    .await;
    assert_eq!(cross.status, StatusCode::FORBIDDEN);
    assert_eq!(
        cross.json(),
        json!({"error": "cross-site writes are not allowed", "kind": "forbidden"})
    );
    assert!(!store.exists("stone/tasks/CS-2.md"));

    let foreign = send(&app, Method::GET, "/api/items", &[("host", "evil.example")], None).await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN);
    assert_eq!(
        foreign.json(),
        json!({"error": "host not allowed", "kind": "forbidden"})
    );

    // The same write from the page the server itself serves goes through.
    let own = send(
        &app,
        Method::POST,
        "/api/items",
        &[("origin", "http://localhost:4747")],
        Some(create),
    )
    .await;
    assert_eq!(own.status, StatusCode::CREATED);
    assert!(store.exists("stone/tasks/CS-2.md"));
}

/// What the table sends (a label, method, uri, headers, body) and what it expects back (status, kind, message part).
type Case<'a> = (
    &'a str,
    Method,
    &'a str,
    &'a [(&'a str, &'a str)],
    Option<&'a str>,
    StatusCode,
    &'a str,
    &'a str,
);

#[tokio::test]
async fn errors() {
    let store = Store::new();
    let first = task("First", "todo", "");
    store.write("stone/tasks/CS-1.md", &first);
    let app = carvestone_server::router(store.repo());
    let cases: [Case; 8] = [
        (
            "get of an unknown id",
            Method::GET,
            "/api/items/CS-99",
            &[],
            None,
            StatusCode::NOT_FOUND,
            "not_found",
            "CS-99 not found",
        ),
        (
            "patch of an unknown id",
            Method::PATCH,
            "/api/items/CS-99",
            &[],
            Some(r#"{"set": {"status": "done"}}"#),
            StatusCode::NOT_FOUND,
            "not_found",
            "CS-99 not found",
        ),
        (
            "patch body that is not JSON",
            Method::PATCH,
            "/api/items/CS-1",
            &[],
            Some("{nope"),
            StatusCode::BAD_REQUEST,
            "bad_request",
            "invalid JSON body",
        ),
        (
            "patch body with an unknown field",
            Method::PATCH,
            "/api/items/CS-1",
            &[],
            Some(r#"{"bogus": 1}"#),
            StatusCode::BAD_REQUEST,
            "bad_request",
            "unknown field `bogus`",
        ),
        (
            "patch with a stale etag",
            Method::PATCH,
            "/api/items/CS-1",
            &[("if-match", "\"stale\"")],
            Some(r#"{"set": {"status": "in-progress"}}"#),
            StatusCode::CONFLICT,
            "conflict",
            "tasks/CS-1 changed on disk; reload",
        ),
        (
            "patch with a blank note",
            Method::PATCH,
            "/api/items/CS-1",
            &[],
            Some(r#"{"set": {"status": "in-progress"}, "note": "  "}"#),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid",
            "the note is empty; write what has to change",
        ),
        (
            "search without a query",
            Method::GET,
            "/api/search",
            &[],
            None,
            StatusCode::BAD_REQUEST,
            "bad_request",
            "missing field `q`",
        ),
        (
            "code with an unknown op",
            Method::GET,
            "/api/code/nope?target=helper",
            &[],
            None,
            StatusCode::BAD_REQUEST,
            "bad_request",
            "unknown code op `nope`",
        ),
    ];
    for (what, method, uri, headers, body, status, kind, message) in cases {
        let answer = send(&app, method, uri, headers, body).await;
        assert_eq!(answer.status, status, "{what}: {}", answer.text);
        let body = answer.json();
        assert_eq!(body["kind"], kind, "{what}: {}", answer.text);
        assert!(
            body["error"].as_str().unwrap().contains(message),
            "{what}: {}",
            answer.text
        );
    }
    // No refused write touched the file.
    assert_eq!(store.read("stone/tasks/CS-1.md"), first);
}
