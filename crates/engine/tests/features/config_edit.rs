//! `config add-code-root` and `config add-scip-index`: the file keeps its comments, its other
//! lines and its line ending, and a bad value is refused before anything is saved.

use crate::support::Store;

#[derive(Clone, Copy)]
enum Act {
    Root(&'static str),
    Scip(&'static str, &'static str, &'static [&'static str]),
}

fn run(
    store: &Store,
    act: Act,
    dry_run: bool,
) -> Result<carvestone_engine::create::Written, carvestone_engine::EngineError> {
    let repo = store.repo();
    match act {
        Act::Root(path) => repo.add_code_root(path, dry_run),
        Act::Scip(file, language, extensions) => {
            let extensions: Vec<String> = extensions.iter().map(|e| e.to_string()).collect();
            repo.add_scip_index(file, language, &extensions, dry_run)
        }
    }
}

#[test]
fn an_entry_is_added_to_the_text_and_a_repeat_changes_nothing() {
    use Act::{Root, Scip};
    let java = Scip(".scip/java.scip", "java", &["java", "kt"]);
    let rows: &[(&str, &str, Act, &str)] = &[
        (
            "key missing",
            "project: t\ntask_prefix: CS\n",
            Root("src"),
            "project: t\ntask_prefix: CS\ncode_roots:\n  - src\n",
        ),
        (
            "key missing, no final newline",
            "project: t",
            Root("src"),
            "project: t\ncode_roots:\n  - src\n",
        ),
        (
            "the repo root is quoted",
            "project: t\n",
            Root("."),
            "project: t\ncode_roots:\n  - '.'\n",
        ),
        (
            "block list",
            "project: t\ncode_roots:\n  - a\n  - b\ntask_prefix: CS\n",
            Root("c"),
            "project: t\ncode_roots:\n  - a\n  - b\n  - c\ntask_prefix: CS\n",
        ),
        (
            "block list at column 0",
            "code_roots:\n- a\ntask_prefix: CS\n",
            Root("c"),
            "code_roots:\n- a\n- c\ntask_prefix: CS\n",
        ),
        (
            "comment lines beside the key",
            "# top\ncode_roots: # where code lives\n  # services\n  - a\n  # tail\ntask_prefix: CS\n",
            Root("c"),
            "# top\ncode_roots: # where code lives\n  # services\n  - a\n  - c\n  # tail\ntask_prefix: CS\n",
        ),
        (
            "inline list",
            "project: t\ncode_roots: [a, 'b c'] # keep\n",
            Root("d"),
            "project: t\ncode_roots: [a, 'b c', d] # keep\n",
        ),
        (
            "empty inline list",
            "code_roots: []\nproject: t\n",
            Root("d"),
            "code_roots: [d]\nproject: t\n",
        ),
        (
            "CRLF, key missing",
            "project: t\r\ntask_prefix: CS\r\n",
            Root("src"),
            "project: t\r\ntask_prefix: CS\r\ncode_roots:\r\n  - src\r\n",
        ),
        (
            "CRLF, block list",
            "code_roots:\r\n  - a\r\nproject: t\r\n",
            Root("b"),
            "code_roots:\r\n  - a\r\n  - b\r\nproject: t\r\n",
        ),
        (
            "scip key missing",
            "project: t\n",
            java,
            "project: t\nscip_indexes:\n  - file: .scip/java.scip\n    language: java\n    extensions: [java, kt]\n",
        ),
        (
            "scip block list after the last entry",
            "scip_indexes:\n  # built in CI\n  - file: a.scip\n    language: go\n    extensions: [go]\nproject: t\n",
            java,
            "scip_indexes:\n  # built in CI\n  - file: a.scip\n    language: go\n    extensions: [go]\n  - file: .scip/java.scip\n    language: java\n    extensions: [java, kt]\nproject: t\n",
        ),
        (
            "scip empty inline list",
            "scip_indexes: [] # none yet\nproject: t\n",
            java,
            "scip_indexes: # none yet\n  - file: .scip/java.scip\n    language: java\n    extensions: [java, kt]\nproject: t\n",
        ),
        (
            "scip CRLF",
            "project: t\r\n",
            java,
            "project: t\r\nscip_indexes:\r\n  - file: .scip/java.scip\r\n    language: java\r\n    extensions: [java, kt]\r\n",
        ),
    ];
    for (name, start, act, expected) in rows {
        let store = Store::new();
        store.write("stone/config.yml", start);

        let dry = run(&store, *act, true).unwrap_or_else(|e| panic!("{name}: dry run: {e}"));
        assert_eq!(dry.text, *expected, "{name}: dry run text");
        assert!(dry.changed, "{name}: dry run reports the change");
        assert_eq!(store.read("stone/config.yml"), *start, "{name}: dry run wrote");

        let done = run(&store, *act, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(done.changed, "{name}");
        assert_eq!(store.read("stone/config.yml"), *expected, "{name}: file");

        let again = run(&store, *act, false).unwrap_or_else(|e| panic!("{name}: repeat: {e}"));
        assert!(!again.changed, "{name}: a repeat changed the file");
        assert_eq!(store.read("stone/config.yml"), *expected, "{name}: repeat file");
    }
}

#[test]
fn a_wrong_entry_is_refused_and_the_file_stays() {
    use Act::{Root, Scip};
    let go = "scip_indexes:\n  - file: a.scip\n    language: go\n    extensions: [go]\n";
    let rows: &[(&str, &str, Act, &str)] = &[
        ("empty root", "project: t\n", Root("  "), "name a folder"),
        ("absolute root", "project: t\n", Root("/abs"), "inside the repo"),
        ("drive root", "project: t\n", Root("C:/x"), "inside the repo"),
        ("parent root", "project: t\n", Root("a/../../b"), "inside the repo"),
        ("a root with a line break", "project: t\n", Root("a\nb"), "read back"),
        (
            "scalar value",
            "code_roots: ~\n",
            Root("a"),
            "`code_roots` in stone/config.yml is not a list stone can extend",
        ),
        (
            "flow list over two lines",
            "code_roots: [a,\n  b]\n",
            Root("c"),
            "is not a list stone can extend; add the entry to it by hand",
        ),
        (
            "scip inline list with entries",
            "scip_indexes: [{file: a.scip, language: go, extensions: [go]}]\n",
            Scip("b.scip", "java", &["java"]),
            "`scip_indexes` in stone/config.yml is not a list stone can extend",
        ),
        (
            "same file, other language",
            go,
            Scip("a.scip", "java", &["java"]),
            "already has an entry for `a.scip` (language `go`, extensions `go`)",
        ),
        (
            "same file, other extensions",
            go,
            Scip("a.scip", "go", &["go", "mod"]),
            "already has an entry for `a.scip`",
        ),
        (
            "file outside the repo",
            "project: t\n",
            Scip("/x.scip", "java", &["java"]),
            "inside the repo",
        ),
        (
            "language project",
            "project: t\n",
            Scip("a.scip", "project", &["java"]),
            "cannot be `project`",
        ),
        (
            "no extensions",
            "project: t\n",
            Scip("a.scip", "java", &[]),
            "`extensions`",
        ),
        (
            "extension with a dot",
            "project: t\n",
            Scip("a.scip", "java", &[".java"]),
            "without a dot",
        ),
    ];
    for (name, start, act, expected) in rows {
        let store = Store::new();
        store.write("stone/config.yml", start);
        for dry_run in [true, false] {
            let err = run(&store, *act, dry_run)
                .err()
                .unwrap_or_else(|| panic!("{name}: dry_run={dry_run} was accepted"))
                .to_string();
            assert!(err.contains(expected), "{name}: `{err}` lacks `{expected}`");
            assert_eq!(store.read("stone/config.yml"), *start, "{name}: the file changed");
        }
    }
}
