//! Search: finding sections by words, through the public `search` function.

use carvestone_engine::EngineError;
use carvestone_engine::edit::Edit;
use carvestone_engine::search::{Hit, SearchQuery, search};
use carvestone_engine::tiers::LeftOut;

use crate::support::Store;

const GUIDE: &str = "---\ntype: doc\ntitle: Guide\n---\n\nIntro text.\n\n## Install\n\nRun the installer with sudo.\n\n## Usage\n\nCall the tool daily.\n";

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        ..SearchQuery::default()
    }
}

fn ids(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

#[test]
fn a_word_finds_its_section_with_anchor_and_lines() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    let repo = store.repo();

    let found = search(&repo, &query("installer")).unwrap();
    assert_eq!(
        found.hits,
        [Hit {
            id: "docs/guide".into(),
            anchor: Some("install".into()),
            path: "stone/docs/guide.md".into(),
            lines: [8, 11],
            kind: "doc".into(),
            snippet: "Run the installer with sudo.".into(),
            exactness: None,
        }]
    );
    assert!(!found.truncated);
    assert_eq!(
        found.hits[0].line(),
        "docs/guide#install  L8-11  doc  Run the installer with sudo."
    );

    let found = search(&repo, &query("daily")).unwrap();
    assert_eq!(ids(&found.hits), ["docs/guide"]);
    assert_eq!(found.hits[0].anchor.as_deref(), Some("usage"));
    assert_eq!(found.hits[0].lines, [12, 14]);
}

#[test]
fn a_title_match_ranks_above_a_body_match() {
    let store = Store::new();
    // The body match sorts first by id, so only the rank can put the title match ahead.
    store.write(
        "stone/docs/alpha.md",
        "---\ntype: doc\ntitle: Notes\n---\n\n## Warm\n\nThe cache is cold.\n",
    );
    store.write("stone/docs/zeta.md", "---\ntype: doc\ntitle: Cache\n---\n");
    let repo = store.repo();

    let found = search(&repo, &query("cache")).unwrap();
    assert_eq!(ids(&found.hits), ["docs/zeta", "docs/alpha"]);
    assert_eq!(found.hits[0].anchor, None);
    assert_eq!(found.hits[1].anchor.as_deref(), Some("warm"));
}

#[test]
fn a_budget_limits_the_output_and_counts_what_it_left_out() {
    let store = Store::new();
    for n in 1..=4 {
        store.write(
            &format!("stone/docs/d{n}.md"),
            &format!("---\ntype: doc\ntitle: D{n}\n---\n\n## Part\n\nshared word here\n"),
        );
    }
    let repo = store.repo();

    // Each printed hit line is 41 chars, so 11 tokens: two fit in 25, a third does not.
    let all = search(&repo, &query("shared")).unwrap();
    assert_eq!(ids(&all.hits), ["docs/d1", "docs/d2", "docs/d3", "docs/d4"]);
    assert_eq!((all.truncated, all.omitted), (false, 0));
    assert_eq!(all.left_out, None);
    assert!(!all.uncounted);

    let two = search(
        &repo,
        &SearchQuery {
            budget: Some(25),
            ..query("shared")
        },
    )
    .unwrap();
    assert_eq!(ids(&two.hits), ["docs/d1", "docs/d2"]);
    assert_eq!((two.truncated, two.omitted), (true, 2));
    // Two lines of 41 chars joined by a newline: 83 chars, 21 tokens.
    let two_out = Some(LeftOut {
        name: "hits".into(),
        lines: 2,
        tokens: 21,
    });
    assert_eq!(two.left_out, two_out);
    assert!(!two.uncounted);

    let one = search(
        &repo,
        &SearchQuery {
            budget: Some(1),
            ..query("shared")
        },
    )
    .unwrap();
    assert_eq!(ids(&one.hits), ["docs/d1"]);
    assert_eq!((one.truncated, one.omitted), (true, 3));
    // Three lines of 41 chars and two newlines: 125 chars, 32 tokens.
    assert_eq!(
        one.left_out,
        Some(LeftOut {
            name: "hits".into(),
            lines: 3,
            tokens: 32,
        })
    );
    assert!(!one.uncounted);

    // The limit counts like the budget.
    let limited = search(
        &repo,
        &SearchQuery {
            limit: 2,
            ..query("shared")
        },
    )
    .unwrap();
    assert_eq!(ids(&limited.hits), ["docs/d1", "docs/d2"]);
    assert_eq!(limited.left_out, two_out);
    assert!(!limited.uncounted);
}

#[test]
fn an_edit_shows_in_the_next_search() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    let repo = store.repo();
    assert_eq!(ids(&search(&repo, &query("installer")).unwrap().hits), ["docs/guide"]);

    let rewrite = Edit {
        section: Some(("Install".into(), "Open the setup wizard.\n\nThen wait.".into())),
        ..Edit::default()
    };
    repo.edit("docs/guide", &rewrite, false).unwrap();
    assert!(search(&repo, &query("installer")).unwrap().hits.is_empty());
    let found = search(&repo, &query("wizard")).unwrap();
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].anchor.as_deref(), Some("install"));
    // The edit adds a `generated:` line to the front matter, so the heading moves down one line.
    assert_eq!(found.hits[0].lines, [9, 14]);

    // A file changed outside stone shows too.
    store.write("stone/docs/guide.md", &GUIDE.replace("daily", "hourly"));
    assert!(search(&repo, &query("daily")).unwrap().hits.is_empty());
    assert_eq!(ids(&search(&repo, &query("hourly")).unwrap().hits), ["docs/guide"]);
}

#[test]
fn errors() {
    let store = Store::new();
    store.write("stone/docs/guide.md", GUIDE);
    let repo = store.repo();
    let from = |id: &str| SearchQuery {
        from: Some(id.into()),
        ..query("installer")
    };
    let cases: [(&str, EngineError, &str); 4] = [
        (
            "empty query",
            search(&repo, &query("")).unwrap_err(),
            "query has no words; give at least one search term",
        ),
        (
            "only punctuation",
            search(&repo, &query("?! --")).unwrap_err(),
            "query has no words; give at least one search term",
        ),
        (
            "unknown from",
            search(&repo, &from("CS-99")).unwrap_err(),
            "CS-99 not found",
        ),
        (
            "unknown doc as from",
            search(&repo, &from("docs/none")).unwrap_err(),
            "docs/none not found",
        ),
    ];
    for (what, err, want) in cases {
        assert!(err.to_string().contains(want), "{what}: {err}");
    }
}
