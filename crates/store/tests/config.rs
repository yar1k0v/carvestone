//! `stone/config.yml` parsing, through the public `Config::parse`.

use carvestone_store::Config;
use carvestone_store::config::ScipIndexEntry;

/// Every shape of `scip_indexes` a user can write wrong, each refused with a sentence that names
/// the fix, and one valid row. A refused `command` key is the point: config never runs anything.
#[test]
fn scip_indexes_table() {
    // (yaml, text the error must contain; empty means the config is valid)
    let rows: [(&str, &str); 14] = [
        (
            "scip_indexes:\n  - language: java\n    extensions: [java]\n",
            "add the `file` key",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    extensions: [java]\n",
            "add the `language` key",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    language: java\n",
            "add the `extensions` key",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    language: java\n    extensions: []\n",
            "non-empty list",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    language: java\n    extensions: [.java]\n",
            "without a dot",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    language: java\n    extensions: [java]\n    command: scip-java index\n",
            "unknown key `command`; only",
        ),
        (
            "scip_indexes:\n  - file: .scip/j.scip\n    language: project\n    extensions: [java]\n",
            "cannot be `project`",
        ),
        (
            "scip_indexes:\n  - file: '..\\j.scip'\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: '\\\\srv\\share\\j.scip'\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: /etc/j.scip\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: 'C:\\idx\\j.scip'\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: ../j.scip\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: .scip/../../j.scip\n    language: java\n    extensions: [java]\n",
            "inside the repo",
        ),
        (
            "scip_indexes:\n  - file: .scip/java.scip\n    language: java\n    extensions: [java, kt]\n",
            "",
        ),
    ];
    for (yaml, expected) in rows {
        let result = Config::parse(yaml);
        if expected.is_empty() {
            let config = result.unwrap_or_else(|err| panic!("{yaml:?} should parse: {err}"));
            assert_eq!(
                config.scip_indexes,
                vec![ScipIndexEntry {
                    file: ".scip/java.scip".into(),
                    language: "java".into(),
                    extensions: vec!["java".into(), "kt".into()],
                }]
            );
        } else {
            let err = result.expect_err(yaml).to_string();
            assert!(err.contains(expected), "{yaml:?}: {err:?} lacks {expected:?}");
        }
    }
    assert!(Config::parse("project: x\n").unwrap().scip_indexes.is_empty());
}
