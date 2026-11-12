use carvestone_engine::tiers::{LeftOut, Part, fit, left_out_line, tokens};

/// Ten lines of 19 characters: the `doc text` part of the worked example.
fn doc_lines() -> Vec<String> {
    (0..10).map(|n| format!("doc text line {n:02}...")).collect()
}

/// Two lines of 9 characters: the `precedents` part of the worked example.
fn precedent_lines() -> Vec<String> {
    (0..2).map(|n| format!("prec {n}...")).collect()
}

/// The worked example's parts: one critical line, then two optional parts.
fn example_parts() -> Vec<Part> {
    let parts = vec![
        Part::critical("task", vec!["task CS-1".to_string()]),
        Part::optional("doc text", None, doc_lines()),
        Part::optional("precedents", None, precedent_lines()),
    ];
    assert!(doc_lines().iter().all(|line| line.chars().count() == 19));
    assert!(precedent_lines().iter().all(|line| line.chars().count() == 9));
    parts
}

#[test]
fn a_budget_cuts_only_optional_parts_and_names_what_it_left_out() {
    let all = format!(
        "task CS-1\n{}\n{}\n",
        doc_lines().join("\n"),
        precedent_lines().join("\n")
    );
    for budget in [None, Some(58)] {
        let fitted = fit(&example_parts(), budget);
        assert_eq!(fitted.text, all, "budget {budget:?}");
        assert_eq!(fitted.text.lines().count(), 13);
        assert!(fitted.left_out.is_empty(), "budget {budget:?}");
    }

    let fitted = fit(&example_parts(), Some(40));
    let cut = "task CS-1\n\
doc text line 00...\n\
doc text line 01...\n\
prec 0...\n\
prec 1...\n\
left out: doc text (8 lines, about 40 tokens); raise --budget\n";
    assert_eq!(fitted.text, cut);
    assert_eq!(
        fitted.left_out,
        vec![LeftOut {
            name: "doc text".to_string(),
            lines: 8,
            tokens: 40,
        }]
    );

    let fitted = fit(&example_parts(), Some(2));
    assert_eq!(
        fitted.text,
        "task CS-1\nleft out: 12 lines, about 55 tokens; raise --budget\n"
    );

    let heading_parts = vec![
        Part::critical("task", vec!["task CS-1".to_string()]),
        Part::optional("doc text", Some("Docs:"), doc_lines()),
        Part::optional("precedents", None, precedent_lines()),
    ];
    let fitted = fit(&heading_parts, Some(40));
    let with_heading = "task CS-1\n\
Docs:\n\
doc text line 00...\n\
doc text line 01...\n\
prec 0...\n\
prec 1...\n\
left out: doc text (8 lines, about 40 tokens); raise --budget\n";
    assert_eq!(fitted.text, with_heading);

    let fitted = fit(&heading_parts, Some(2));
    assert_eq!(
        fitted.text,
        "task CS-1\nleft out: 12 lines, about 55 tokens; raise --budget\n"
    );

    let empty_heading = vec![
        Part::critical("task", vec!["task CS-1".to_string()]),
        Part::optional("empty", Some("Empty:"), Vec::new()),
    ];
    for budget in [None, Some(2)] {
        let fitted = fit(&empty_heading, budget);
        assert_eq!(fitted.text, "task CS-1\n", "budget {budget:?}");
        assert!(fitted.left_out.is_empty(), "budget {budget:?}");
    }
}

#[test]
fn every_budget_keeps_the_critical_part_and_the_guarantee() {
    let parts = example_parts();
    for budget in 1..=80 {
        let fitted = fit(&parts, Some(budget));
        assert!(
            fitted.text.starts_with("task CS-1\n"),
            "budget {budget}: {}",
            fitted.text
        );
        assert!(
            tokens(&fitted.text) <= budget.max(16),
            "budget {budget}: {} tokens",
            tokens(&fitted.text)
        );
        let kept = fitted.text.lines().count() - 1 - usize::from(!fitted.left_out.is_empty());
        let dropped: usize = fitted.left_out.iter().map(|out| out.lines).sum();
        assert_eq!(kept + dropped, 12, "budget {budget}: {}", fitted.text);
    }
}

#[test]
fn the_left_out_line_names_what_was_cut_and_its_size() {
    let hits = LeftOut {
        name: "hits".to_string(),
        lines: 2,
        tokens: 21,
    };
    let entries = LeftOut {
        name: "entries".to_string(),
        lines: 1,
        tokens: 9,
    };
    let rows = [
        (
            &hits,
            false,
            "--limit or --budget",
            "left out: hits (2 lines, about 21 tokens); raise --limit or --budget",
        ),
        (
            &entries,
            false,
            "--budget",
            "left out: entries (1 line, about 9 tokens); raise --budget",
        ),
        (
            &hits,
            true,
            "--limit or --budget",
            "left out: hits (2 lines, about 21 tokens) and more not counted; raise --limit or --budget",
        ),
    ];
    for (left, more, raise, want) in rows {
        assert_eq!(left_out_line(left, more, raise), want);
    }
}
