//! Merging two edits of one stone item: by paragraph and by front matter field.

use carvestone_engine::merge::merge_text;

const FRONT: &str = "---\ntitle: T\nstatus: todo\nlinks: [a]\n---\n";

fn item(body: &str) -> String {
    format!("{FRONT}{body}")
}

/// (name, base, ours, theirs, expected text, expected conflicts)
fn rows() -> Vec<(&'static str, String, String, String, String, usize)> {
    let three = "P1\n\nP2\n\nP3\n";
    let fence = "```\nx\n\ny\n```\n\nafter\n";
    let crlf = |s: &str| s.replace('\n', "\r\n");
    let status = |v: &str| format!("---\ntitle: T\nstatus: {v}\nlinks: [a]\n---\n");
    let links = |v: &str| format!("---\ntitle: T\nstatus: todo\nlinks: [{v}]\n---\nP1\n");
    vec![
        ("no change", item(three), item(three), item(three), item(three), 0),
        (
            "ours edits P1",
            item(three),
            item("P1 o\n\nP2\n\nP3\n"),
            item(three),
            item("P1 o\n\nP2\n\nP3\n"),
            0,
        ),
        (
            "ours edits P1, theirs edits P3",
            item(three),
            item("P1 o\n\nP2\n\nP3\n"),
            item("P1\n\nP2\n\nP3 t\n"),
            item("P1 o\n\nP2\n\nP3 t\n"),
            0,
        ),
        (
            "both edit P1 differently",
            item(three),
            item("P1 o\n\nP2\n\nP3\n"),
            item("P1 t\n\nP2\n\nP3\n"),
            item("<<<<<<< ours\nP1 o\n\n=======\nP1 t\n\n>>>>>>> theirs\nP2\n\nP3\n"),
            1,
        ),
        (
            "both edit P1 the same",
            item(three),
            item("P1 s\n\nP2\n\nP3\n"),
            item("P1 s\n\nP2\n\nP3\n"),
            item("P1 s\n\nP2\n\nP3\n"),
            0,
        ),
        (
            "both insert the same paragraph at one place",
            item("P1\n\nP2\n"),
            item("P1\n\nNEW\n\nP2\n"),
            item("P1\n\nNEW\n\nP2\n"),
            item("P1\n\nNEW\n\nP2\n"),
            0,
        ),
        (
            "both append at the end",
            item("P1\n"),
            item("P1\n\nO\n"),
            item("P1\n\nT\n"),
            item("P1\n\nO\n\nT\n"),
            0,
        ),
        (
            "fence with a blank line edited on one side",
            item(fence),
            item("```\nx\n\nz\n```\n\nafter\n"),
            item("```\nx\n\ny\n```\n\nafter t\n"),
            item("```\nx\n\nz\n```\n\nafter t\n"),
            0,
        ),
        (
            "CRLF stays CRLF",
            crlf(&item(three)),
            crlf(&item("P1 o\n\nP2\n\nP3\n")),
            crlf(&item("P1\n\nP2 t\n\nP3\n")),
            crlf(&item("P1 o\n\nP2 t\n\nP3\n")),
            0,
        ),
        (
            "links added on both sides",
            links("a"),
            links("a, b"),
            links("a, c"),
            links("a, b, c"),
            0,
        ),
        (
            "status changed on both sides",
            format!("{}P1\n", status("todo")),
            format!("{}P1\n", status("in-progress")),
            format!("{}P1\n", status("review")),
            format!(
                "<<<<<<< ours\n{}=======\n{}>>>>>>> theirs\nP1\n",
                status("in-progress"),
                status("review")
            ),
            1,
        ),
        (
            "unparsable front matter on one side",
            item("P1\n"),
            "no front matter\n\nP1\n".to_string(),
            item("P1 t\n"),
            format!(
                "<<<<<<< ours\nno front matter\n\nP1\n=======\n{}>>>>>>> theirs\n",
                item("P1 t\n")
            ),
            1,
        ),
    ]
}

#[test]
fn merge_table() {
    for (name, base, ours, theirs, want, conflicts) in rows() {
        let got = merge_text(&base, &ours, &theirs);
        assert_eq!(got.text, want, "{name}");
        assert_eq!(got.conflicts, conflicts, "{name}: conflicts");
    }
}

#[test]
fn merge_is_identity() {
    for (name, base, ours, theirs, _, _) in rows() {
        for side in [&ours, &theirs] {
            assert_eq!(&merge_text(&base, &base, side).text, side, "{name}: base = ours");
            assert_eq!(&merge_text(&base, side, &base).text, side, "{name}: base = theirs");
        }
    }
}
