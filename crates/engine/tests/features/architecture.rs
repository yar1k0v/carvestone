//! Architecture: the design views (`hld`, `lld`), `scan`, `map` and drift, over one small shop.

use carvestone_engine::arch_gaps::{GapKind, gaps};
use carvestone_engine::arch_graph::{DriftKind, arch_graph};
use carvestone_engine::design::{Connection, DesignDoc, DrawnRel, Part, hld, lld};
use carvestone_engine::scan::{ScanOpts, ScanState, scan};
use carvestone_engine::service_map::ServiceCode;

use crate::support::Store;

const CARGO: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

const API: &str = "---\ntype: container\ntitle: Api\nparent: arch/shop\ntechnology: Rust\ndescription: Serves orders.\nuses: [arch/shop/store]\ncode: [src/api.rs, src/lib.rs]\n---\n\n## Uses\n\n- [[arch/shop/store]]: Saves orders\n";
const STORE: &str =
    "---\ntype: container\ntitle: Store\nparent: arch/shop\ntechnology: SQLite\ncode: [src/store.rs]\n---\n";
const HANDLERS: &str = "---\ntype: component\ntitle: Handlers\nparent: arch/shop/api\ncode: [src/handlers.rs]\n---\n";

/// A shop with two containers and one component. `src/api.rs` calls `src/store.rs` and
/// `src/jobs.rs` calls it too, but nothing owns `jobs.rs`. The component owns `src/handlers.rs`.
/// The crate root `src/lib.rs` has to be there for the calls to resolve; the api owns it.
fn shop() -> Store {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write(
        "stone/arch/shop.md",
        "---\ntype: system\ntitle: Shop\ndescription: Sells things online.\n---\n\n## Context\n\n```mermaid\nC4Context\n  title Shop\n  Rel(api, store, \"reads orders\")\n```\n",
    );
    store.write("stone/arch/shop/api.md", API);
    store.write("stone/arch/shop/store.md", STORE);
    store.write("stone/arch/shop/api/handlers.md", HANDLERS);
    store.write("Cargo.toml", CARGO);
    store.write(
        "src/lib.rs",
        "pub mod api;\npub mod handlers;\npub mod jobs;\npub mod store;\n",
    );
    store.write("src/handlers.rs", "pub fn ping() -> u32 {\n    0\n}\n");
    store.write("src/store.rs", "pub fn save() -> u32 {\n    1\n}\n");
    store.write(
        "src/api.rs",
        "use crate::store::save;\n\npub fn handle() -> u32 {\n    save()\n}\n",
    );
    store.write(
        "src/jobs.rs",
        "use crate::store::save;\n\npub fn nightly() -> u32 {\n    save()\n}\n",
    );
    store.git_track();
    store
}

/// The text without lines that start with `prefix`, for dates that change every day.
fn without(text: &str, prefix: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| !line.starts_with(prefix))
        .collect()
}

fn part(id: &str, title: &str, type_name: &str, technology: Option<&str>, parent: Option<&str>) -> Part {
    Part {
        id: id.into(),
        title: title.into(),
        type_name: type_name.into(),
        description: None,
        technology: technology.map(Into::into),
        parent: parent.map(Into::into),
        kind: None,
    }
}

/// Scans the shop once and returns the path of the one draft it wrote, relative to `stone/`.
fn drafted(store: &Store) -> String {
    let report = scan(&store.repo(), &ScanOpts::default()).unwrap();
    assert_eq!(report.entries.len(), 1, "{:?}", report.entries);
    report.entries[0].0.clone()
}

#[test]
fn hld_shows_the_systems_containers_connections_and_marked_docs() {
    let store = shop();
    store.write(
        "stone/docs/overview.md",
        "---\ntype: doc\ntitle: Shop overview\nlevel: hld\nkind: overview\n---\n\nHow the shop fits together.\n",
    );
    store.write(
        "stone/docs/notes.md",
        "---\ntype: doc\ntitle: Notes\n---\n\nUnmarked.\n",
    );
    store.write(
        "stone/arch/shop/store.md",
        &STORE.replace("technology: SQLite\n", "technology: SQLite\nkind: store\n"),
    );

    let found = hld(&store.repo()).unwrap();

    let mut shop = part("arch/shop", "Shop", "system", None, None);
    shop.description = Some("Sells things online.".into());
    let mut api = part("arch/shop/api", "Api", "container", Some("Rust"), Some("arch/shop"));
    api.description = Some("Serves orders.".into());
    let mut db = part(
        "arch/shop/store",
        "Store",
        "container",
        Some("SQLite"),
        Some("arch/shop"),
    );
    db.kind = Some("store".into());
    // The `handlers` component is in neither list: the high-level view stops at containers.
    assert_eq!(found.systems, [shop]);
    assert_eq!(found.containers, [api, db]);
    assert_eq!(
        found.connections,
        [Connection {
            from: "arch/shop/api".into(),
            to: "arch/shop/store".into(),
            description: Some("Saves orders".into()),
        }]
    );
    assert_eq!(
        found.drawn,
        [DrawnRel {
            item: "arch/shop".into(),
            line: "Rel(api, store, \"reads orders\")".into(),
        }]
    );
    assert_eq!(
        found.docs,
        [DesignDoc {
            id: "docs/overview".into(),
            title: "Shop overview".into(),
            start: None,
            kind: Some("overview".into()),
            summary: Some("How the shop fits together.".into()),
            updated: None,
        }]
    );
}

#[test]
fn lld_shows_one_container_with_its_code_and_the_low_level_docs_that_link_it() {
    let store = shop();
    store.write(
        "stone/docs/lld-api.md",
        "---\ntype: doc\ntitle: Api flow\nlevel: lld\nkind: flow\n---\n\nHow an order is handled, see [[arch/shop/api]].\n",
    );
    store.write(
        "stone/docs/lld-store.md",
        "---\ntype: doc\ntitle: Store flow\nlevel: lld\n---\n\nHow an order is saved, see [[arch/shop/store]].\n",
    );
    store.write(
        "stone/docs/api-notes.md",
        "---\ntype: doc\ntitle: Api notes\n---\n\nSee [[arch/shop/api]].\n",
    );

    let found = lld(&store.repo(), "arch/shop/api").unwrap();

    let mut api = part("arch/shop/api", "Api", "container", Some("Rust"), Some("arch/shop"));
    api.description = Some("Serves orders.".into());
    assert_eq!(found.container, api);
    let service = found.service.expect("a container with code has a service view");
    assert_eq!(service.target, "arch/shop/api");
    let files: Vec<String> = service.files.iter().map(|hit| hit.path.clone()).collect();
    assert_eq!(files, ["src/api.rs", "src/lib.rs"]);
    // The day of the last change is today's date, which a test cannot spell out.
    let updated = found.docs[0].updated.clone();
    assert_eq!(updated.as_deref().map(str::len), Some(10));
    assert_eq!(
        found.docs,
        [DesignDoc {
            id: "docs/lld-api".into(),
            title: "Api flow".into(),
            start: None,
            kind: Some("flow".into()),
            summary: Some("How an order is handled, see api.".into()),
            updated,
        }]
    );

    // Without code there is no service view, but the container is still shown.
    store.write(
        "stone/arch/shop/api.md",
        &API.replace("code: [src/api.rs, src/lib.rs]\n", ""),
    );
    assert!(lld(&store.repo(), "arch/shop/api").unwrap().service.is_none());
}

#[test]
fn scan_writes_a_draft_for_code_no_component_owns() {
    let store = shop();
    let repo = store.repo();

    let dry = scan(
        &repo,
        &ScanOpts {
            dry_run: true,
            ..ScanOpts::default()
        },
    )
    .unwrap();
    assert_eq!((dry.written, dry.entries.len()), (1, 1));
    assert!(!store.exists("stone/arch/scan"));
    assert!(!store.exists(".stone/scan-state.json"));

    let report = scan(&repo, &ScanOpts::default()).unwrap();
    assert_eq!(report.written, 1);
    let (key, state) = &report.entries[0];
    assert!(key.starts_with("arch/scan/demo-src-") && key.ends_with(".md"), "{key}");
    assert_eq!(*state, ScanState::New);

    // Only the file nobody owns is drafted, so neither the containers' files nor the component's
    // `src/handlers.rs` is in it; the draft uses what their code calls.
    let text = store.read(&format!("stone/{key}"));
    let want = "---\ntype: component\ntitle: 'demo: src'\nstatus: draft\ntechnology: Rust\ncode: ['src/jobs.rs']\nuses: ['arch/shop/store']\n---\n\n## Responsibilities\n\nDraft from `stone scan`: 1 files under `src` in project demo. Say what they are for, curate `code`, `entry` and `uses`, then change the status.\n";
    assert_eq!(without(&without(&text, "created:"), "generated:"), want);
    assert!(store.exists(".stone/scan-state.json"));

    // A second scan over the same code writes nothing.
    let again = scan(&repo, &ScanOpts::default()).unwrap();
    assert_eq!(again.written, 0);
    assert_eq!(again.entries, [(key.clone(), ScanState::Unchanged)]);
    assert_eq!(store.read(&format!("stone/{key}")), text);
}

#[test]
fn a_draft_follows_the_code_unless_a_human_edited_it() {
    // (case, a human edits the draft first, state, drafts written, draft stays as it was)
    let cases = [
        ("nobody edited it", false, ScanState::Changed, 1, false),
        ("a human edited it", true, ScanState::KeptEdited, 0, true),
    ];
    for (what, human_edit, state, written, kept) in cases {
        let store = shop();
        let key = drafted(&store);
        let path = format!("stone/{key}");
        if human_edit {
            let edited = format!("{}\nOur own notes.\n", store.read(&path));
            store.write(&path, &edited);
        }
        let before = store.read(&path);

        // The job no longer calls the store, so the scan would write a draft without that use.
        store.write("src/jobs.rs", "pub fn nightly() -> u32 {\n    2\n}\n");
        let report = scan(&store.repo(), &ScanOpts::default()).unwrap();

        assert_eq!(report.entries, [(key, state)], "{what}");
        assert_eq!(report.written, written, "{what}");
        let after = store.read(&path);
        if kept {
            assert_eq!(after, before, "{what}");
        } else {
            assert!(after.contains("\nuses: []\n"), "{what}: {after}");
        }
    }
}

#[test]
fn a_draft_no_code_needs_any_more_is_stale_and_pruned_on_request() {
    let store = shop();
    let key = drafted(&store);
    let path = format!("stone/{key}");

    // The owner takes the file over, so the draft has nothing left to describe.
    store.write(
        "stone/arch/shop/api.md",
        &API.replace("src/lib.rs]", "src/lib.rs, src/jobs.rs]"),
    );
    let stale = scan(&store.repo(), &ScanOpts::default()).unwrap();
    assert_eq!(stale.entries, [(key.clone(), ScanState::Stale)]);
    assert!(store.exists(&path));

    let pruned = scan(
        &store.repo(),
        &ScanOpts {
            prune: true,
            ..ScanOpts::default()
        },
    )
    .unwrap();
    assert_eq!(pruned.entries, [(key, ScanState::Pruned)]);
    assert!(!store.exists(&path));
}

/// Three services with no code: `billing` has one project folder named like it, `orders` has two,
/// `ghost` has none. `cart` already owns its folder.
fn services() -> Store {
    let store = Store::new();
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots: [services/]\n",
    );
    store.write("stone/arch/shop.md", "---\ntype: system\ntitle: Shop\n---\n");
    for name in ["billing", "orders", "ghost"] {
        store.write(
            &format!("stone/arch/shop/{name}.md"),
            &format!(
                "---\ntype: container\ntitle: {}\nparent: arch/shop\n---\n",
                name.to_uppercase()
            ),
        );
    }
    store.write(
        "stone/arch/shop/cart.md",
        "---\ntype: container\ntitle: Cart\nparent: arch/shop\ncode: ['services/cart/**']\n---\n",
    );
    store.write("services/billing/package.json", "{\"name\": \"billing\"}\n");
    store.write("services/orders/package.json", "{\"name\": \"orders\"}\n");
    store.write("services/legacy/orders/package.json", "{\"name\": \"orders-legacy\"}\n");
    store.write("services/cart/package.json", "{\"name\": \"cart\"}\n");
    store.git_track();
    store
}

#[test]
fn map_proposes_the_folder_named_like_each_service_without_code_and_saves_the_pick() {
    let store = services();

    let found = store.repo().service_code().unwrap();

    let service =
        |id: &str, title: &str, folder: Option<&str>, project: Option<&str>, candidates: &[&str]| ServiceCode {
            id: id.into(),
            title: title.into(),
            folder: folder.map(Into::into),
            project: project.map(Into::into),
            candidates: candidates.iter().map(|c| c.to_string()).collect(),
        };
    assert_eq!(
        found,
        [
            service(
                "arch/shop/billing",
                "BILLING",
                Some("services/billing"),
                Some("services/billing/package.json"),
                &[]
            ),
            service("arch/shop/ghost", "GHOST", None, None, &[]),
            service(
                "arch/shop/orders",
                "ORDERS",
                None,
                None,
                &["services/legacy/orders", "services/orders"]
            ),
        ]
    );

    // Saving a pick writes the folder as a recursive glob, and the service leaves the list.
    let repo = store.repo();
    repo.set_service_code(&[("arch/shop/orders".into(), "services/orders/".into())])
        .unwrap();

    let text = store.read("stone/arch/shop/orders.md");
    assert!(text.contains("\ncode: [services/orders/**]\n"), "{text}");
    let left: Vec<String> = repo.service_code().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(left, ["arch/shop/billing", "arch/shop/ghost"]);
}

#[test]
fn drift_is_silent_while_the_design_and_the_code_agree_and_reports_each_difference() {
    let found = arch_graph(&shop().repo()).unwrap();
    assert!(found.drift.is_empty(), "{:?}", found.drift);
    assert_eq!(found.code_edges.len(), 1);
    assert_eq!(found.code_edges[0].summary(), "2 edges, first src/api.rs:1");

    // (case, file that changes, its new text, kind, from, to, the warning)
    let cases = [
        (
            "a code dependency the design does not list",
            "stone/arch/shop/api.md",
            API.replace("uses: [arch/shop/store]\n", ""),
            DriftKind::Code,
            "arch/shop/api",
            "arch/shop/store",
            "stone/arch/shop/api.md:7: arch-drift: arch/shop/api depends on arch/shop/store in code (2 edges, first src/api.rs:1) but does not list it in uses",
        ),
        (
            "a uses entry no code backs",
            "stone/arch/shop/store.md",
            STORE.replace("code:", "uses: [arch/shop/api]\ncode:"),
            DriftKind::Uses,
            "arch/shop/store",
            "arch/shop/api",
            "stone/arch/shop/store.md:6: arch-drift: arch/shop/store lists arch/shop/api in uses but no code edge exists",
        ),
    ];
    for (what, path, text, kind, from, to, warning) in cases {
        let store = shop();
        store.write(path, &text);

        let found = arch_graph(&store.repo()).unwrap().drift;

        assert_eq!(found.len(), 1, "{what}: {found:?}");
        assert_eq!(
            (found[0].kind, found[0].from.as_str(), found[0].to.as_deref()),
            (kind, from, Some(to)),
            "{what}"
        );
        assert_eq!(found[0].problem.to_string(), warning, "{what}");
    }
}

const FILES: &str = "---\ntype: component\ntitle: Files\nparent: arch/shop/store\n---\n";

#[test]
fn connect_writes_the_uses_entry_and_the_bullet_and_a_repeat_changes_nothing() {
    let store = shop();
    store.write("stone/arch/shop/store/files.md", FILES);
    // A CRLF item file: every line connect writes or replaces has to keep the CRLF.
    store.write("stone/arch/shop/api/handlers.md", &HANDLERS.replace('\n', "\r\n"));
    let repo = store.repo();
    let lone_lf = |text: &str| text.replace("\r\n", "").contains('\n');

    let first = repo
        .connect(
            "arch/shop/api/handlers",
            "arch/shop/store/files",
            "Writes blobs",
            Some("NATS"),
            false,
        )
        .unwrap();

    assert!(first.changed);
    let raw = store.read("stone/arch/shop/api/handlers.md");
    assert!(!lone_lf(&raw), "{raw:?}");
    let text = raw.replace("\r\n", "\n");
    assert!(text.contains("\nuses: [arch/shop/store/files]\n"), "{text}");
    assert!(text.contains("\ngenerated:"), "{text}");
    assert!(
        text.contains("\n## Uses\n\n- [[arch/shop/store/files]]: Writes blobs (over NATS)\n"),
        "{text}"
    );
    // The code has no edge between the two, and the link runs over NATS, so nothing drifts.
    assert!(arch_graph(&store.repo()).unwrap().drift.is_empty());

    // `hld` lists connections between systems and containers only.
    repo.connect("arch/shop/store", "arch/shop/api", "Sends events", Some("NATS"), false)
        .unwrap();
    let found = hld(&store.repo()).unwrap();
    assert!(
        found.connections.contains(&Connection {
            from: "arch/shop/store".into(),
            to: "arch/shop/api".into(),
            description: Some("Sends events".into()),
        }),
        "{:?}",
        found.connections
    );

    let again = repo
        .connect(
            "arch/shop/api/handlers",
            "arch/shop/store/files",
            "Writes blobs",
            Some("NATS"),
            false,
        )
        .unwrap();
    assert!(!again.changed);
    assert_eq!(store.read("stone/arch/shop/api/handlers.md"), raw);

    // A second target adds one line; a new phrase for the first target replaces only its own bullet.
    repo.connect("arch/shop/api/handlers", "arch/shop/store", "Reads stock", None, false)
        .unwrap();
    repo.connect(
        "arch/shop/api/handlers",
        "arch/shop/store/files",
        "Saves blobs",
        None,
        false,
    )
    .unwrap();
    let raw = store.read("stone/arch/shop/api/handlers.md");
    assert!(!lone_lf(&raw), "{raw:?}");
    let text = raw.replace("\r\n", "\n");
    assert!(
        text.ends_with("\n## Uses\n\n- [[arch/shop/store/files]]: Saves blobs\n- [[arch/shop/store]]: Reads stock\n"),
        "{text}"
    );
    assert_eq!(text.matches("arch/shop/store/files]]").count(), 1, "{text}");
}

#[test]
fn connect_refuses_what_is_not_a_connection_between_two_parts() {
    let store = shop();
    store.write("stone/arch/shop/store/files.md", FILES);
    store.write(
        "stone/tasks/CS-1.md",
        "---\ntype: task\ntitle: Do it\nstatus: todo\n---\n",
    );
    let repo = store.repo();
    let nine = "one two three four five six seven eight nine";

    // (case, from, to, does, over, the words the refusal must hold)
    let cases = [
        (
            "from is missing",
            "arch/shop/nope",
            "arch/shop/store",
            "Saves",
            None,
            "`arch/shop/nope` is not in the store; name an existing system, container or component",
        ),
        (
            "to is missing",
            "arch/shop/api",
            "arch/shop/nope",
            "Saves",
            None,
            "`arch/shop/nope` is not in the store; name an existing system, container or component",
        ),
        (
            "does is only a protocol mark",
            "arch/shop/api",
            "arch/shop/store",
            "(over NATS)",
            None,
            "would not read back as written",
        ),
        (
            "does ends in a known protocol",
            "arch/shop/api",
            "arch/shop/store",
            "(HTTP)",
            None,
            "would not read back as written",
        ),
        (
            "does ends in a protocol and over is not given",
            "arch/shop/api",
            "arch/shop/store",
            "Reads items (HTTP)",
            None,
            "would not read back as written",
        ),
        (
            "does holds a link",
            "arch/shop/api",
            "arch/shop/store",
            "Reads [[arch/nope]]",
            None,
            "holds `[[`",
        ),
        (
            "from is a task",
            "CS-1",
            "arch/shop/store",
            "Saves",
            None,
            "is not a system, container or component",
        ),
        (
            "to is a task",
            "arch/shop/api",
            "CS-1",
            "Saves",
            None,
            "is not a system, container or component",
        ),
        (
            "from is to",
            "arch/shop/api",
            "arch/shop/api",
            "Saves",
            None,
            "cannot connect to itself",
        ),
        (
            "does is empty",
            "arch/shop/api",
            "arch/shop/store",
            "  ",
            None,
            "`--does` is empty",
        ),
        (
            "does is long",
            "arch/shop/api",
            "arch/shop/store",
            nine,
            None,
            "`--does` has 9 words",
        ),
        (
            "over is empty",
            "arch/shop/api",
            "arch/shop/store",
            "Saves",
            Some(" "),
            "`--over` is empty",
        ),
        (
            "over holds a link",
            "arch/shop/api",
            "arch/shop/store",
            "Reads",
            Some("[[arch/nope]]"),
            "`--over` holds `[[`",
        ),
        (
            "over holds a parenthesis",
            "arch/shop/api",
            "arch/shop/store",
            "Saves",
            Some("NATS)"),
            "holds a parenthesis",
        ),
    ];
    // Every row is checked before the test fails, so one run names all the rows that are wrong.
    let mut wrong = Vec::new();
    for (what, from, to, does, over, want) in cases {
        match repo.connect(from, to, does, over, false) {
            Err(err) if err.to_string().contains(want) => {}
            other => wrong.push(format!("{what}: {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // A refusal writes nothing.
    assert_eq!(store.read("stone/arch/shop/api.md"), API);
}

/// Added: no other test reaches a `## Uses` section that is not a tidy list.
#[test]
fn connect_changes_only_its_own_lines_in_a_untidy_uses_section() {
    let head = "---\ntype: component\ntitle: Handlers\nparent: arch/shop/api\ncode: [src/handlers.rs]\nuses: [arch/shop/store/files]\n---\n";
    let files = "- [[arch/shop/store/files]]: Writes blobs";
    let stock = "- [[arch/shop/store]]: Reads stock";
    // (case, the body before, the body after connecting to arch/shop/store with `Reads stock`)
    let cases = [
        (
            "no blank line after the heading, two after the bullets",
            format!("\n## Uses\n{files}\n\n\nTail.\n"),
            format!("\n## Uses\n{files}\n{stock}\n\n\nTail.\n"),
        ),
        (
            "a sub-heading after the bullets",
            format!("\n## Uses\n\n{files}\n\n### Sub\n\nProse.\n"),
            format!("\n## Uses\n\n{files}\n{stock}\n\n### Sub\n\nProse.\n"),
        ),
        (
            "an indented line under the bullet that is replaced",
            format!("\n## Uses\n\n- [[arch/shop/store]]: Old text\n  more old text\n{files}\n"),
            format!("\n## Uses\n\n{stock}\n{files}\n"),
        ),
        (
            "a nested bullet under the bullet that is replaced",
            "\n## Uses\n\n- [[arch/shop/store]]: Old text\n  - [[arch/shop/store/files]]: Writes blobs\n".to_string(),
            format!("\n## Uses\n\n{stock}\n  {files}\n"),
        ),
        (
            "a list indented by two spaces",
            format!("\n## Uses\n\n  - [[arch/shop/store]]: Old text\n  {files}\n  - [[arch/shop/api]]: Serves\n"),
            format!("\n## Uses\n\n  {stock}\n  {files}\n  - [[arch/shop/api]]: Serves\n"),
        ),
        (
            "a heading with no bullet under it",
            "\n## Uses\n\n## Other\n\nText.\n".to_string(),
            format!("\n## Uses\n\n{stock}\n\n## Other\n\nText.\n"),
        ),
    ];
    let mut wrong = Vec::new();
    for (what, before, after) in cases {
        let store = shop();
        store.write("stone/arch/shop/store/files.md", FILES);
        store.write("stone/arch/shop/api/handlers.md", &format!("{head}{before}"));

        store
            .repo()
            .connect("arch/shop/api/handlers", "arch/shop/store", "Reads stock", None, false)
            .unwrap();

        let text = store.read("stone/arch/shop/api/handlers.md");
        if !text.ends_with(&format!("\n---\n{after}")) {
            wrong.push(format!("{what}: {text:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Added beside the drift test because no `uses_bullets` test exists. It reads the bullet the way
/// the drift check does, so a stack the remote list does not name must still mark a run-time link.
#[test]
fn a_connection_that_leaves_the_process_is_never_drift() {
    // (bullet text, whether the missing code edge is reported)
    let cases = [
        ("Sends frames (over NATS)", false),
        ("Reads items (HTTP)", false),
        ("Parses ids (internal)", true),
        ("Sends frames (overnight)", true),
    ];
    for (bullet, drifts) in cases {
        let store = shop();
        store.write(
            "stone/arch/shop/store.md",
            &format!(
                "{}uses: [arch/shop/api]\n---\n\n## Uses\n\n- [[arch/shop/api]]: {bullet}\n",
                STORE.replace("code: [src/store.rs]\n---\n", "code: [src/store.rs]\n")
            ),
        );

        let found = arch_graph(&store.repo()).unwrap().drift;

        assert_eq!(found.len(), usize::from(drifts), "{bullet}: {found:?}");
    }
}

/// One container of each trouble and one that is complete. `api` is clean. `store` has no doc,
/// `queue` has no connection, `ghost` has no code and lists `batch`, `store` (over HTTP) and `api`
/// (over gRPC over TLS) with no phrase, `batch` calls
/// `store` in code but lists no `uses` for it, and nothing owns `src/jobs.rs`. `bucket` is a data
/// store with no code and no doc, which is complete.
fn gappy() -> Store {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    store.write("stone/arch/shop.md", "---\ntype: system\ntitle: Shop\n---\n");
    let part = |name: &str, extra: &str| {
        store.write(
            &format!("stone/arch/shop/{name}.md"),
            &format!("---\ntype: container\ntitle: {name}\nparent: arch/shop\n{extra}---\n"),
        );
    };
    store.write(
        "stone/arch/shop/api.md",
        "---\ntype: container\ntitle: api\nparent: arch/shop\nuses: [arch/shop/store]\ncode: [src/api.rs, src/lib.rs]\n---\n\n## Uses\n\n- [[arch/shop/store]]: Saves items\n",
    );
    part("store", "code: [src/store.rs]\n");
    part("queue", "code: [src/queue.rs]\n");
    store.write(
        "stone/arch/shop/ghost.md",
        "---\ntype: container\ntitle: ghost\nparent: arch/shop\nuses: [arch/shop/batch, arch/shop/store, arch/shop/api]\n---\n\n## Uses\n\n- [[arch/shop/store]]: (HTTP)\n- [[arch/shop/api]]: (over gRPC over TLS)\n",
    );
    part("batch", "code: [src/batch.rs]\n");
    store.write(
        "stone/docs/design.md",
        "---\ntype: doc\ntitle: Design\n---\n\nSee [[arch/shop/api]], [[arch/shop/queue]], [[arch/shop/ghost]] and [[arch/shop/batch]].\n",
    );
    store.write("Cargo.toml", CARGO);
    store.write(
        "src/lib.rs",
        "pub mod api;\npub mod batch;\npub mod jobs;\npub mod queue;\npub mod store;\n",
    );
    store.write("src/store.rs", "pub fn save() -> u32 {\n    1\n}\n");
    store.write("src/queue.rs", "pub fn push() -> u32 {\n    0\n}\n");
    // A store with no code and no doc: it still needs a connection, and this one runs over NATS.
    store.write(
        "stone/arch/shop/bucket.md",
        "---\ntype: container\ntitle: bucket\nparent: arch/shop\nkind: store\nuses: [arch/shop/api]\n---\n\n## Uses\n\n- [[arch/shop/api]]: Sends frames (over NATS)\n",
    );
    for name in ["api", "batch", "jobs"] {
        store.write(
            &format!("src/{name}.rs"),
            "use crate::store::save;\n\npub fn run() -> u32 {\n    save()\n}\n",
        );
    }
    store.git_track();
    store
}

#[test]
fn gaps_list_what_the_design_lacks_with_the_next_command_and_skip_what_is_complete() {
    let store = gappy();
    let found = gaps(&store.repo()).unwrap();

    let got: Vec<(GapKind, &str, &str)> = found
        .items
        .iter()
        .map(|g| (g.kind, g.id.as_str(), g.next.as_str()))
        .collect();
    let connect = "stone arch connect arch/shop/batch arch/shop/store --does <text>";
    assert_eq!(
        got,
        [
            (GapKind::NoCode, "arch/shop/ghost", "stone map --write"),
            (GapKind::UnownedCode, "arch/scan/demo-src-7bb69d.md", "stone scan"),
            (
                GapKind::NoConnection,
                "arch/shop/queue",
                "stone arch connect arch/shop/queue <to> --does <text>"
            ),
            (
                GapKind::NoPhrase,
                "arch/shop/ghost",
                "stone arch connect arch/shop/ghost arch/shop/batch --does <text>"
            ),
            (
                GapKind::NoPhrase,
                "arch/shop/ghost",
                "stone arch connect arch/shop/ghost arch/shop/store --does <text> --over HTTP"
            ),
            (
                GapKind::NoPhrase,
                "arch/shop/ghost",
                "stone arch connect arch/shop/ghost arch/shop/api --does <text> --over \"gRPC over TLS\""
            ),
            (
                GapKind::NoDoc,
                "arch/shop/store",
                "stone new doc --title <title> --link arch/shop/store"
            ),
            (GapKind::ContradictedLink, "arch/shop/batch", connect),
        ]
    );
    assert!(
        found.items[7].detail.contains("depends on arch/shop/store in code"),
        "{:?}",
        found.items[7]
    );
    // Reading gaps writes nothing, not even the draft.
    assert!(!store.exists("stone/arch/scan"));

    // The same store gives the same list.
    assert_eq!(gaps(&store.repo()).unwrap(), found);
}

#[test]
fn a_run_time_connection_with_no_code_edge_is_not_a_contradicted_link() {
    let store = gappy();
    store.write(
        "stone/arch/shop/queue.md",
        "---\ntype: container\ntitle: queue\nparent: arch/shop\nuses: [arch/shop/api]\ncode: [src/queue.rs]\n---\n\n## Uses\n\n- [[arch/shop/api]]: Sends frames (over NATS)\n",
    );

    let found = gaps(&store.repo()).unwrap();

    let queue: Vec<GapKind> = found
        .items
        .iter()
        .filter(|g| g.id == "arch/shop/queue")
        .map(|g| g.kind)
        .collect();
    assert!(queue.is_empty(), "{queue:?}");
    assert!(
        !found
            .items
            .iter()
            .any(|g| g.kind == GapKind::ContradictedLink && g.id != "arch/shop/batch")
    );
}

/// Added: no other test reaches the `kind` and `parent` rules of `stone check`. Every wrong case is a
/// row; the last row is a store, an outside system and a product that are all right.
#[test]
fn check_refuses_a_kind_on_the_wrong_type_and_a_parent_that_breaks_the_product_rule() {
    let person = "---\ntype: system\ntitle: User\nkind: person\n---\n";
    // (case, files written over the shop, the `kind` and `parent` findings that must come back)
    let cases = [
        (
            "a system takes no store kind",
            vec![("stone/arch/odd.md", "---\ntype: system\ntitle: Odd\nkind: store\n---\n")],
            vec!["stone/arch/odd.md:4: kind: kind `store` is not valid for a system; use one of: person, external"],
        ),
        (
            "a container takes no person kind",
            vec![(
                "stone/arch/shop/odd.md",
                "---\ntype: container\ntitle: Odd\nparent: arch/shop\nkind: person\n---\n",
            )],
            vec!["stone/arch/shop/odd.md:5: kind: kind `person` is not valid for a container; use one of: store"],
        ),
        (
            "a component takes no kind",
            vec![(
                "stone/arch/shop/api/odd.md",
                "---\ntype: component\ntitle: Odd\nparent: arch/shop/api\nkind: store\n---\n",
            )],
            vec![
                "stone/arch/shop/api/odd.md:5: kind: kind `store` is not valid for a component; a component takes no kind",
            ],
        ),
        (
            "a person sits in no product",
            vec![(
                "stone/arch/user.md",
                "---\ntype: system\ntitle: User\nparent: arch/shop\nkind: person\n---\n",
            )],
            vec!["stone/arch/user.md:4: parent: a system with kind `person` cannot have a parent; remove `parent`"],
        ),
        (
            "a system's parent is a container",
            vec![(
                "stone/arch/sub.md",
                "---\ntype: system\ntitle: Sub\nparent: arch/shop/api\n---\n",
            )],
            vec![
                "stone/arch/sub.md:4: parent: parent `arch/shop/api` is not a system; a system's parent must be a system",
            ],
        ),
        (
            "a system with a kind is never a parent",
            vec![
                ("stone/arch/user.md", person),
                (
                    "stone/arch/shop/odd.md",
                    "---\ntype: container\ntitle: Odd\nparent: arch/user\n---\n",
                ),
            ],
            vec![
                "stone/arch/shop/odd.md:4: parent: parent `arch/user` is a system with kind `person`; only a system with no kind can be a parent",
            ],
        ),
        (
            "a store, an outside system and a product are all right",
            vec![
                (
                    "stone/arch/shop/store.md",
                    "---\ntype: container\ntitle: Store\nparent: arch/shop\nkind: store\n---\n",
                ),
                (
                    "stone/arch/bank.md",
                    "---\ntype: system\ntitle: Bank\nkind: external\n---\n",
                ),
                ("stone/arch/user.md", person),
                ("stone/arch/suite.md", "---\ntype: system\ntitle: Suite\n---\n"),
                (
                    "stone/arch/shop.md",
                    "---\ntype: system\ntitle: Shop\nparent: arch/suite\n---\n",
                ),
            ],
            vec![],
        ),
    ];
    // Every row is checked before the test fails, so one run names all the rows that are wrong.
    let mut wrong = Vec::new();
    for (what, files, want) in cases {
        let store = shop();
        for (path, text) in files {
            store.write(path, text);
        }
        let found: Vec<String> = store
            .repo()
            .check()
            .unwrap()
            .iter()
            .filter(|p| matches!(p.rule, "kind" | "parent"))
            .map(ToString::to_string)
            .collect();
        if found != want {
            wrong.push(format!("{what}: {found:#?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn errors() {
    let designed = shop();
    let none = Store::new();
    let services = services();
    let broken = shop();
    broken.write(".stone/scan-state.json", "{not json");
    let cases: [(&str, String, &str); 6] = [
        (
            "lld of a system",
            lld(&designed.repo(), "arch/shop").unwrap_err().to_string(),
            "`arch/shop` is not a container; the containers are: arch/shop/api, arch/shop/store",
        ),
        (
            "lld in a store without containers",
            lld(&none.repo(), "arch/shop").unwrap_err().to_string(),
            "this store has no containers; add one with `stone new container`",
        ),
        (
            "map folder that does not exist",
            services
                .repo()
                .set_service_code(&[("arch/shop/billing".into(), "services/nope".into())])
                .unwrap_err()
                .to_string(),
            "`services/nope` is not a folder under",
        ),
        (
            "map service that already has code",
            services
                .repo()
                .set_service_code(&[("arch/shop/cart".into(), "services/billing".into())])
                .unwrap_err()
                .to_string(),
            "already names its code; change its `code` list by hand",
        ),
        (
            "map onto a system",
            services
                .repo()
                .set_service_code(&[("arch/shop".into(), "services/billing".into())])
                .unwrap_err()
                .to_string(),
            "is a system; only containers and components hold code",
        ),
        (
            "scan state that is not json",
            scan(&broken.repo(), &ScanOpts::default()).unwrap_err().to_string(),
            "cannot read .stone/scan-state.json: ",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.contains(want), "{what}: {err}");
    }
}
