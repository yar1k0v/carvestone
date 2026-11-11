//! The built-in guide: every `stone` command a topic names is one the CLI has.

use carvestone_core::{code_span_ranges, lines};
use carvestone_engine::guide::{CLI_COMMANDS, CLI_NESTED, Topic, guide};

/// The command in a backticked `stone ...` span that the CLI does not have, or `None`.
/// A flag or a `<placeholder>` after `stone` names nothing, and a word after a command with
/// subcommands must be one of them.
fn wrong_command(span: &str) -> Option<String> {
    let mut words = span.split_whitespace();
    if words.next() != Some("stone") {
        return None;
    }
    let name = words.next().filter(|w| !w.starts_with(['-', '<', '[']))?;
    if !CLI_COMMANDS.contains(&name) {
        return Some(name.to_string());
    }
    if !CLI_NESTED.iter().any(|(parent, _)| *parent == name) {
        return None;
    }
    let child = words
        .next()
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-'))?;
    (!CLI_NESTED.contains(&(name, child))).then(|| format!("{name} {child}"))
}

/// The `stone` command spans of a guide text, outside code fences.
fn stone_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    for (_, line, in_fence) in lines(text, 1) {
        if in_fence {
            continue;
        }
        for (from, to) in code_span_ranges(line) {
            let span = line[from..to].trim_matches('`').trim();
            if span.starts_with("stone ") {
                spans.push(span.to_string());
            }
        }
    }
    spans
}

#[test]
fn every_command_in_a_guide_topic_exists() {
    let mut wrong = Vec::new();
    for topic in Topic::ALL {
        for span in stone_spans(guide(topic)) {
            if let Some(command) = wrong_command(&span) {
                wrong.push(format!("{}: `{span}` names {command}", topic.name()));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "guides name commands the CLI lacks:\n{}",
        wrong.join("\n")
    );

    // The checker itself: a missing command and a missing subcommand are both caught.
    for (span, expected) in [
        ("stone nope", Some("nope")),
        ("stone arch nope", Some("arch nope")),
        ("stone arch gaps", None),
        ("stone config add-code-root <path>", None),
        ("stone --help", None),
    ] {
        assert_eq!(wrong_command(span).as_deref(), expected, "{span}");
    }
}

#[test]
fn design_topic_gives_six_steps_that_end_in_a_check() {
    let text = guide(Topic::from_name("design").expect("design is a topic"));
    let steps: Vec<&str> = text
        .lines()
        .filter(|l| l.as_bytes().first().is_some_and(u8::is_ascii_digit))
        .collect();
    assert_eq!(steps.len(), 6, "{steps:?}");
    assert!(
        steps[5].starts_with("6. Finish") && steps[5].contains("`stone check`"),
        "{}",
        steps[5]
    );
}
