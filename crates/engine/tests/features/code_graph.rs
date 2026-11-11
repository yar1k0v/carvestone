//! Code graph: find, uses, impact, path and outline over small projects written inline.

use carvestone_engine::EngineError;
use carvestone_engine::code::{CodeQuery, OutlineEntry, deps, find, outline, uses};
use carvestone_engine::code_graph::{impact, path};
use carvestone_engine::code_native::{NativeQuery, native};
use carvestone_engine::search::{Hit, SearchQuery, search};

use crate::support::Store;

fn query(target: &str) -> CodeQuery {
    CodeQuery {
        target: target.into(),
        ..CodeQuery::default()
    }
}

/// The lines a user sees, one per hit.
fn lines(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(Hit::line).collect()
}

fn entries(list: &[OutlineEntry]) -> Vec<String> {
    list.iter()
        .map(|e| {
            format!(
                "{}{} {} {} L{}-{}",
                "  ".repeat(e.depth as usize),
                e.qname,
                e.kind,
                e.short,
                e.lines[0],
                e.lines[1]
            )
        })
        .collect()
}

/// A store whose code lives under `src/`.
fn project(files: &[(&str, &str)]) -> Store {
    let store = Store::new();
    store.write("stone/config.yml", "project: t\ntask_prefix: CS\ncode_roots: [src/]\n");
    for (rel, text) in files {
        store.write(rel, text);
    }
    store
}

/// Rust edges need the crate manifest: it tells which crate a file belongs to.
const CARGO: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

/// `start` calls `app::run`, which calls `util::helper`; `Runner::total` calls nothing.
fn rust_project() -> Store {
    project(&[
        ("Cargo.toml", CARGO),
        (
            "src/lib.rs",
            "pub mod app;\npub mod util;\n\npub fn start() -> u32 {\n    app::run()\n}\n",
        ),
        ("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n"),
        (
            "src/app.rs",
            "use crate::util::helper;\n\npub struct Runner {\n    pub count: u32,\n}\n\nimpl Runner {\n    pub fn total(&self) -> u32 {\n        self.count\n    }\n}\n\npub fn run() -> u32 {\n    helper()\n}\n",
        ),
    ])
}

#[test]
fn each_language_finds_a_definition_and_its_call_site_in_another_file() {
    // (language, files, name, find lines, uses lines)
    type Row<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str, &'a [&'a str], &'a [&'a str]);
    let rows: [Row; 5] = [
        (
            "rust",
            &[
                ("Cargo.toml", CARGO),
                ("src/lib.rs", "pub mod app;\npub mod util;\n"),
                ("src/util.rs", "pub fn helper() -> u32 {\n    1\n}\n"),
                (
                    "src/app.rs",
                    "use crate::util::helper;\n\npub fn run() -> u32 {\n    helper()\n}\n",
                ),
            ],
            "helper",
            &["util::helper  src/util.rs:L1-3  function  exact  pub fn helper() -> u32"],
            &[
                "src/app.rs  src/app.rs:L1-5  imports  exact  @L1",
                "app::run  src/app.rs:L3-5  calls  exact  @L4 pub fn run() -> u32",
            ],
        ),
        (
            "typescript",
            &[
                ("src/util.ts", "export function helper(): number {\n  return 1;\n}\n"),
                (
                    "src/app.ts",
                    "import { helper } from \"./util\";\n\nexport function run(): number {\n  return helper();\n}\n",
                ),
            ],
            "helper",
            &["src/util::helper  src/util.ts:L1-3  function  exact  export function helper(): number"],
            &[
                "src/app.ts  src/app.ts:L1-5  imports  exact  @L1",
                "src/app::run  src/app.ts:L3-5  calls  exact  @L4 export function run(): number",
            ],
        ),
        (
            "python",
            &[
                ("src/util.py", "def helper():\n    return 1\n"),
                (
                    "src/app.py",
                    "from util import helper\n\n\ndef run():\n    return helper()\n",
                ),
            ],
            "helper",
            &["util.helper  src/util.py:L1-2  function  exact  def helper()"],
            &[
                "src/app.py  src/app.py:L1-5  imports  exact  @L1",
                "app.run  src/app.py:L4-5  calls  exact  @L5 def run()",
            ],
        ),
        (
            "csharp",
            &[
                (
                    "src/Util.cs",
                    "namespace Demo\n{\n    public static class Util\n    {\n        public static int Helper()\n        {\n            return 1;\n        }\n    }\n}\n",
                ),
                (
                    "src/App.cs",
                    "using Demo;\n\nnamespace Demo\n{\n    public class App\n    {\n        public int Run()\n        {\n            return Util.Helper();\n        }\n    }\n}\n",
                ),
            ],
            "Helper",
            &["Demo.Util.Helper  src/Util.cs:L5-8  method  exact  public static int Helper()"],
            &["Demo.App.Run  src/App.cs:L7-10  calls  exact  @L9 public int Run()"],
        ),
        (
            "dart",
            &[
                ("src/lib/util.dart", "int helper() {\n  return 1;\n}\n"),
                (
                    "src/lib/app.dart",
                    "import 'util.dart';\n\nint run() {\n  return helper();\n}\n",
                ),
            ],
            "helper",
            &["src/lib/util.dart#helper  src/lib/util.dart:L1-3  function  exact  int helper()"],
            &["src/lib/app.dart#run  src/lib/app.dart:L3-5  calls  exact  @L4 int run()"],
        ),
    ];
    for (lang, files, name, want_find, want_uses) in rows {
        let store = project(files);
        let repo = store.repo();
        assert_eq!(
            lines(&find(&repo, &query(name)).unwrap().hits),
            want_find,
            "{lang} find"
        );
        assert_eq!(
            lines(&uses(&repo, &query(name)).unwrap().hits),
            want_uses,
            "{lang} uses"
        );
    }
}

#[test]
fn uses_of_an_implementers_method_lists_the_interface_member_that_dispatches_to_it() {
    let store = project(&[
        ("src/worker.ts", "export interface Worker {\n  run(): number;\n}\n"),
        (
            "src/impl.ts",
            "import { Worker } from \"./worker\";\n\nexport class Impl implements Worker {\n  run(): number {\n    return 1;\n  }\n}\n",
        ),
    ]);
    let repo = store.repo();
    let hits = lines(&uses(&repo, &query("src/impl::Impl.run")).unwrap().hits);
    assert!(
        hits.iter()
            .any(|hit| hit.starts_with("src/worker::Worker.run") && hit.contains("dispatch")),
        "{hits:?}"
    );
}

#[test]
fn outline_lists_the_symbols_of_a_file_or_of_one_symbol_with_lines() {
    let store = rust_project();
    let repo = store.repo();

    let file = outline(&repo, &query("src/app.rs")).unwrap();
    assert_eq!(file.path, "src/app.rs");
    assert_eq!(
        entries(&file.entries),
        [
            "app::Runner type struct Runner L3-5",
            "  app::Runner::total method total() : u32 L8-10",
            "app::run function run() : u32 L13-15",
        ]
    );

    // One symbol shows itself and what it holds, nothing else of the file.
    let one = outline(&repo, &query("app::Runner")).unwrap();
    assert_eq!(
        entries(&one.entries),
        [
            "app::Runner type struct Runner L3-5",
            "  app::Runner::total method total() : u32 L8-10"
        ]
    );
}

#[test]
fn impact_reaches_the_caller_of_a_caller() {
    let store = rust_project();
    let repo = store.repo();

    let found = impact(&repo, &query("helper")).unwrap();
    assert_eq!(
        lines(&found.dependents),
        [
            "src/app.rs  src/app.rs:L1-15  file  exact  depth 1",
            "app::run  src/app.rs:L13-15  function  exact  depth 1 pub fn run() -> u32",
            "start  src/lib.rs:L4-6  function  exact  depth 2 pub fn start() -> u32",
        ]
    );

    let near = impact(
        &repo,
        &CodeQuery {
            depth: Some(1),
            ..query("helper")
        },
    )
    .unwrap();
    assert_eq!(lines(&near.dependents), lines(&found.dependents[..2]));
}

#[test]
fn path_joins_two_symbols_in_either_order_and_says_when_none_does() {
    let store = rust_project();
    let repo = store.repo();
    let between = |from: &str, to: &str| {
        path(
            &repo,
            &CodeQuery {
                to: Some(to.into()),
                ..query(from)
            },
        )
        .unwrap()
    };

    let forward = between("start", "helper");
    assert_eq!((forward.direction.as_str(), forward.exact), ("forward", true));
    assert_eq!(
        lines(&forward.steps),
        [
            "start  src/lib.rs:L4-6  function  exact  start pub fn start() -> u32",
            "app::run  src/app.rs:L13-15  function  exact  calls pub fn run() -> u32",
            "util::helper  src/util.rs:L1-3  function  exact  calls pub fn helper() -> u32",
        ]
    );

    // Asked backwards, the chain is still walked from the caller.
    let reverse = between("helper", "start");
    assert_eq!(reverse.direction, "reverse");
    assert_eq!(lines(&reverse.steps), lines(&forward.steps));

    let none = between("total", "helper");
    assert_eq!((none.direction.as_str(), none.steps.len()), ("none", 0));
}

#[test]
fn a_changed_file_shows_in_the_next_query() {
    let store = rust_project();
    let repo = store.repo();
    assert_eq!(uses(&repo, &query("helper")).unwrap().hits.len(), 2);
    assert!(find(&repo, &query("fresh")).unwrap().hits.is_empty());

    store.write(
        "src/app.rs",
        "pub fn run() -> u32 {\n    2\n}\n\npub fn fresh() -> u32 {\n    3\n}\n",
    );
    assert_eq!(
        lines(&find(&repo, &query("fresh")).unwrap().hits),
        ["app::fresh  src/app.rs:L5-7  function  exact  pub fn fresh() -> u32"]
    );
    assert!(uses(&repo, &query("helper")).unwrap().hits.is_empty());
    assert_eq!(
        entries(&outline(&repo, &query("src/app.rs")).unwrap().entries),
        [
            "app::run function run() : u32 L1-3",
            "app::fresh function fresh() : u32 L5-7"
        ]
    );
}

/// A file of a language only `scip_indexes` lists has one file symbol and no declarations, keeps
/// it through a refresh, loses it with the file, and a listed index that was never built is
/// reported, not an error.
#[test]
fn a_scip_listed_file_has_one_file_symbol_and_a_missing_index_is_reported() {
    let store = project(&[("src/A.java", "class A {}\n")]);
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots: [src/]\nscip_indexes:\n  - file: .scip/java.scip\n    language: java\n    extensions: [java]\n",
    );
    let repo = store.repo();
    let files = CodeQuery {
        kind: Some("file".into()),
        ..query("A")
    };
    let want = ["src/A.java  src/A.java:L1-1  file  exact"];
    assert_eq!(lines(&find(&repo, &files).unwrap().hits), want);
    assert!(
        entries(&outline(&repo, &query("src/A.java")).unwrap().entries).is_empty(),
        "a listed file has no declarations until its index is read"
    );

    // A changed file is parsed again and still has its one symbol.
    store.write("src/A.java", "class A {}\nclass B {}\n");
    assert_eq!(
        lines(&find(&repo, &files).unwrap().hits),
        ["src/A.java  src/A.java:L1-2  file  exact"]
    );

    std::fs::remove_file(store.path("src/A.java")).unwrap();
    assert!(find(&repo, &files).unwrap().hits.is_empty());

    // No indexer is found and `.scip/java.scip` does not exist.
    let report = native(
        &repo,
        &NativeQuery {
            search_path: Some("".into()),
            ..NativeQuery::default()
        },
    )
    .unwrap();
    let row = report
        .indexers
        .iter()
        .find(|run| run.indexer == ".scip/java.scip")
        .expect("the missing index is named in the report");
    assert_eq!(row.status, "skipped");
    assert_eq!(row.detail, ".scip/java.scip not found; run your indexer first");
}

/// A tool that stops with an object dump (its last line is `}`) is reported by its error line, the
/// full output is kept in a file the report names, and scip-python is never left to ask git for
/// a version.
#[test]
fn a_failed_indexer_reports_its_error_line_and_where_its_output_is() {
    let store = project(&[("src/a.py", "def a():\n    return 1\n")]);
    let (name, text) = if cfg!(windows) {
        (
            "scip-python.cmd",
            "@echo off\r\necho %* 1>&2\r\necho Experienced Fatal Error While Indexing: 1>&2\r\necho   currentFilepath: 'src/a.py', 1>&2\r\necho   error: RangeError: Maximum call stack size exceeded 1>&2\r\necho } 1>&2\r\nexit /b 1\r\n",
        )
    } else {
        (
            "scip-python",
            "#!/bin/sh\necho \"$@\" >&2\necho 'Experienced Fatal Error While Indexing:' >&2\necho \"  currentFilepath: 'src/a.py',\" >&2\necho '  error: RangeError: Maximum call stack size exceeded' >&2\necho '}' >&2\nexit 1\n",
        )
    };
    store.write(&format!("bin/{name}"), text);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            store.path(&format!("bin/{name}")),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let report = native(
        &store.repo(),
        &NativeQuery {
            search_path: Some(store.path("bin").into_os_string()),
            ..NativeQuery::default()
        },
    )
    .unwrap();
    let run = report
        .indexers
        .iter()
        .find(|run| run.indexer == "scip-python")
        .expect("the failed indexer is in the report");
    assert_eq!(run.status, "failed");
    assert_eq!(
        run.detail,
        "scip-python in .: error: RangeError: Maximum call stack size exceeded; full output: .stone/native/scip-python-0.log"
    );
    let log = store.read(".stone/native/scip-python-0.log");
    assert!(log.contains("--project-version 0"), "{log}");
    assert!(log.contains("currentFilepath: 'src/a.py'"), "{log}");
}

/// One occurrence: `range` and `enclosing` are SCIP's 0-based forms.
fn occurrence(symbol: &str, range: [i32; 3], enclosing: Option<[i32; 4]>) -> scip::types::Occurrence {
    scip::types::Occurrence {
        range: range.to_vec(),
        symbol: symbol.into(),
        symbol_roles: if enclosing.is_some() {
            scip::types::SymbolRole::Definition as i32
        } else {
            0
        },
        enclosing_range: enclosing.map(|e| e.to_vec()).unwrap_or_default(),
        ..Default::default()
    }
}

/// A definition whose end line is in `typed_enclosing_range` only, the way scip-go writes it.
fn typed_definition(symbol: &str, range: [i32; 3], enclosing: [i32; 4]) -> scip::types::Occurrence {
    use scip::types::{MultiLineRange, occurrence::Typed_enclosing_range};
    scip::types::Occurrence {
        range: range.to_vec(),
        symbol: symbol.into(),
        symbol_roles: scip::types::SymbolRole::Definition as i32,
        typed_enclosing_range: Some(Typed_enclosing_range::MultiLineEnclosingRange(MultiLineRange {
            start_line: enclosing[0],
            start_character: enclosing[1],
            end_line: enclosing[2],
            end_character: enclosing[3],
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// A hand-built index of three Java files: `App.run` calls `Greeter.greet`, and in `Plain.java`,
/// which has only typed enclosing ranges, `second` calls `first`. `greet_used` keeps the call,
/// `greet_defined` keeps the method (without it the call has nothing to point at).
fn java_index(greet_defined: bool, greet_used: bool) -> Vec<u8> {
    use protobuf::Message;
    const GREETER: &str = "semanticdb maven . . Greeter#";
    const GREET: &str = "semanticdb maven . . Greeter#greet().";
    const APP: &str = "semanticdb maven . . App#";
    const RUN: &str = "semanticdb maven . . App#run().";
    let doc = |path: &str, occurrences| scip::types::Document {
        relative_path: path.into(),
        language: "java".into(),
        occurrences,
        ..Default::default()
    };
    let mut greeter = vec![occurrence(GREETER, [0, 6, 13], Some([0, 0, 4, 1]))];
    if greet_defined {
        greeter.push(occurrence(GREET, [1, 11, 16], Some([1, 4, 3, 5])));
    }
    let mut app = vec![
        occurrence(APP, [0, 6, 9], Some([0, 0, 4, 1])),
        occurrence(RUN, [1, 9, 12], Some([1, 4, 3, 5])),
        occurrence(GREETER, [2, 12, 19], None),
    ];
    if greet_used {
        app.push(occurrence(GREET, [2, 22, 27], None));
    }
    let plain = vec![
        typed_definition("semanticdb maven . . plain/first().", [0, 5, 10], [0, 0, 1, 1]),
        typed_definition("semanticdb maven . . plain/second().", [2, 5, 11], [2, 0, 4, 1]),
        occurrence("semanticdb maven . . plain/first().", [3, 4, 9], None),
    ];
    let index = scip::types::Index {
        documents: vec![
            doc("src/Greeter.java", greeter),
            doc("src/App.java", app),
            doc("src/Plain.java", plain),
        ],
        ..Default::default()
    };
    index.write_to_bytes().unwrap()
}

/// A language only `scip_indexes` lists gets its symbols from the index: found, outlined with
/// their lines and nested, and the same after a second pass.
#[test]
fn a_scip_listed_language_gets_its_symbols_from_the_index() {
    let store = project(&[
        (
            "src/Greeter.java",
            "class Greeter {\n    String greet() {\n        return \"hi\";\n    }\n}\n",
        ),
        (
            "src/App.java",
            "class App {\n    void run() {\n        new Greeter().greet();\n    }\n}\n",
        ),
        (
            "src/Plain.java",
            "void first() {\n}\nvoid second() {\n    first();\n}\n",
        ),
    ]);
    store.write(
        "stone/config.yml",
        "project: t\ntask_prefix: CS\ncode_roots: [src/]\nscip_indexes:\n  - file: .scip/java.scip\n    language: java\n    extensions: [java]\n",
    );
    std::fs::create_dir_all(store.path(".scip")).unwrap();
    let write_index = |greet_defined, greet_used| {
        std::fs::write(store.path(".scip/java.scip"), java_index(greet_defined, greet_used)).unwrap();
    };
    // The first index has the method but not the call to it.
    write_index(true, false);
    let repo = store.repo();
    let pass = || {
        native(
            &repo,
            &NativeQuery {
                search_path: Some("".into()),
                ..NativeQuery::default()
            },
        )
        .unwrap()
    };
    // Before the pass a listed file has only its file symbol.
    assert!(entries(&outline(&repo, &query("src/Greeter.java")).unwrap().entries).is_empty());

    assert_eq!(pass().documents, 3);
    let kind = |name: &str, kind: &str| CodeQuery {
        kind: Some(kind.into()),
        ..query(name)
    };
    assert_eq!(
        lines(&find(&repo, &kind("Greeter", "type")).unwrap().hits),
        ["Greeter  src/Greeter.java:L1-5  type  exact"]
    );
    assert_eq!(
        lines(&find(&repo, &kind("greet", "method")).unwrap().hits),
        ["Greeter.greet  src/Greeter.java:L2-4  method  exact"]
    );
    assert_eq!(
        entries(&outline(&repo, &query("src/Greeter.java")).unwrap().entries),
        ["Greeter type Greeter L1-5", "  Greeter.greet method greet L2-4"]
    );

    assert!(lines(&uses(&repo, &query("Greeter.greet")).unwrap().hits).is_empty());

    // A changed index that adds the call: same symbols, one more site.
    write_index(true, true);
    pass();
    // `run` holds line 3, where `greet` is named.
    let callers = ["App.run  src/App.java:L2-4  references  exact  @L3"];
    assert_eq!(lines(&uses(&repo, &query("Greeter.greet")).unwrap().hits), callers);
    // An index with only `typed_enclosing_range` still gives `second` its body, so the call to
    // `first` names the function and not the file.
    assert_eq!(
        lines(&uses(&repo, &query("plain.first")).unwrap().hits),
        ["plain.second  src/Plain.java:L3-5  references  exact  @L4"]
    );
    let version = || -> String {
        repo.index()
            .unwrap()
            .conn()
            .query_row("SELECT value FROM meta WHERE key = 'graph_version'", [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    let before = version();

    // The same index again leaves the same rows and does not move the graph.
    pass();
    assert_eq!(version(), before);
    assert_eq!(lines(&uses(&repo, &query("Greeter.greet")).unwrap().hits), callers);
    assert_eq!(lines(&find(&repo, &kind("Greeter", "type")).unwrap().hits).len(), 1);
    assert_eq!(
        entries(&outline(&repo, &query("src/Greeter.java")).unwrap().entries),
        ["Greeter type Greeter L1-5", "  Greeter.greet method greet L2-4"]
    );

    // A changed index: the call to `greet` is gone, the method stays.
    let greet_found = || {
        let hits = find(&repo, &kind("greet", "method")).unwrap().hits;
        let searched = search(
            &repo,
            &SearchQuery {
                text: "greet".into(),
                ..SearchQuery::default()
            },
        )
        .unwrap()
        .hits;
        (lines(&hits), searched.iter().any(|hit| hit.id == "Greeter.greet"))
    };
    write_index(true, false);
    pass();
    assert_ne!(version(), before);
    assert!(lines(&uses(&repo, &query("Greeter.greet")).unwrap().hits).is_empty());
    assert_eq!(
        greet_found(),
        (
            vec!["Greeter.greet  src/Greeter.java:L2-4  method  exact".to_string()],
            true
        )
    );
    let after_call = version();
    pass();
    assert_eq!(version(), after_call);

    // Then the method is gone too.
    write_index(false, false);
    pass();
    assert_ne!(version(), after_call);
    let gone = uses(&repo, &query("Greeter.greet")).unwrap_err().to_string();
    assert!(gone.starts_with("no symbol or file matches `Greeter.greet`"), "{gone}");
    assert_eq!(greet_found(), (Vec::new(), false));
    assert_eq!(
        entries(&outline(&repo, &query("src/Greeter.java")).unwrap().entries),
        ["Greeter type Greeter L1-5"]
    );
}

#[test]
fn find_says_when_no_code_roots_are_set() {
    let store = Store::new();
    store.write("src/lib.rs", "pub fn helper() -> u32 {\n    1\n}\n");
    let found = find(&store.repo(), &query("helper")).unwrap();

    assert!(found.hits.is_empty());
    assert_eq!(
        found.note.as_deref(),
        Some(
            "no code is indexed, because stone/config.yml has no code_roots; add code_roots: [.] to index the whole repo"
        )
    );
}

#[test]
fn an_empty_answer_says_what_is_not_indexed() {
    let store = rust_project();
    let repo = store.repo();
    let between = |from: &str, to: &str| {
        path(
            &repo,
            &CodeQuery {
                to: Some(to.into()),
                ..query(from)
            },
        )
        .unwrap()
    };
    // (command, gap, first line, last line)
    let rows = [
        (
            "uses",
            uses(&repo, &query("total")).unwrap().gap,
            "indexed for rust: declarations",
            "next: git grep -n -F -e \"total\"",
        ),
        (
            "deps",
            deps(&repo, &query("total")).unwrap().gap,
            "indexed for rust: declarations",
            "next: git grep -n -F -e \"total\"",
        ),
        (
            "impact",
            impact(&repo, &query("total")).unwrap().gap,
            "indexed for rust: declarations",
            "next: git grep -n -F -e \"total\"",
        ),
        (
            "path",
            between("total", "helper").gap,
            "indexed for rust: declarations",
            "next: git grep -n -F -e \"total\" -e \"helper\"",
        ),
        (
            "find",
            find(&repo, &query("zzz")).unwrap().gap,
            "indexed here: declarations of rust",
            "next: git grep -n -F -e \"zzz\"",
        ),
    ];
    for (command, gap, first, last) in rows {
        let gap = gap.unwrap_or_else(|| panic!("{command} has no gap"));
        let lines: Vec<&str> = gap.lines().collect();
        assert_eq!(lines.len(), 3, "{command}: {gap}");
        assert!(lines[0].starts_with(first), "{command}: {}", lines[0]);
        assert_eq!(lines[1], "not indexed: string literals, config keys", "{command}");
        assert_eq!(lines[2], last, "{command}: {gap}");
    }
    // A filled answer says nothing about its gap: the query text already answered it.
    assert_eq!(uses(&repo, &query("helper")).unwrap().gap, None);
}

#[test]
fn errors() {
    let store = rust_project();
    store.write("src/lib.rs", "pub mod app;\npub mod other;\npub mod util;\n");
    store.write("src/other.rs", "pub fn helper() -> u32 {\n    2\n}\n");
    let repo = store.repo();
    let kind = CodeQuery {
        kind: Some("banana".into()),
        ..query("helper")
    };
    let cases: [(&str, EngineError, &str); 6] = [
        (
            "empty find",
            find(&repo, &query(" ")).unwrap_err(),
            "give a name or a prefix to find",
        ),
        (
            "unknown kind",
            find(&repo, &kind).unwrap_err(),
            "no symbol kind `banana`; use one of: type, method",
        ),
        (
            "unknown symbol",
            uses(&repo, &query("nothing")).unwrap_err(),
            "no symbol or file matches `nothing`; try `stone code find nothing`",
        ),
        (
            "two symbols share a name",
            outline(&repo, &query("helper")).unwrap_err(),
            "helper matches 2 symbols; use one of: other::helper, util::helper",
        ),
        (
            "no target",
            impact(&repo, &query("")).unwrap_err(),
            "give a symbol, a file path or path:line",
        ),
        (
            "path with one end",
            path(&repo, &query("run")).unwrap_err(),
            "give two targets: stone code path <a> <b>",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
}
