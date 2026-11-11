//! The workflow guide `stone instructions` prints. It teaches the CLI only.
//!
//! One markdown file per topic under `guide/`; the exported lists below let the
//! CLI test check the names in the overview against its parser.

use std::path::Path;

use carvestone_core::{Section, code_span_ranges, lines, section_text, sections};

use crate::check::Problem;
use crate::repo::Repo;
use crate::resolve::Snapshot;

/// A guide topic, named on the command line as `overview`, `plan`, `do`, `finish`, `debug` or `design`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Overview,
    Plan,
    Do,
    Finish,
    Debug,
    Design,
}

impl Topic {
    /// Every topic.
    pub const ALL: [Topic; 6] = [
        Topic::Overview,
        Topic::Plan,
        Topic::Do,
        Topic::Finish,
        Topic::Debug,
        Topic::Design,
    ];

    /// The name the CLI argument uses.
    pub fn name(self) -> &'static str {
        match self {
            Topic::Overview => "overview",
            Topic::Plan => "plan",
            Topic::Do => "do",
            Topic::Finish => "finish",
            Topic::Debug => "debug",
            Topic::Design => "design",
        }
    }

    pub fn from_name(name: &str) -> Option<Topic> {
        Topic::ALL.into_iter().find(|topic| topic.name() == name)
    }
}

/// The sentence a name outside [`Topic`] gets.
pub fn unknown_topic(name: &str) -> String {
    format!("unknown guide {name}; use overview, plan, do, finish, debug or design")
}

/// The text of one guide.
pub fn guide(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => OVERVIEW,
        Topic::Plan => PLAN,
        Topic::Do => DO,
        Topic::Finish => FINISH,
        Topic::Debug => DEBUG,
        Topic::Design => DESIGN,
    }
}

/// The id of the doc in which a project writes its own guide: one section per topic, headed with
/// the topic's name in any letter case. A topic with a section there replaces the built-in text.
pub const PROJECT_DOC: &str = "docs/instructions";

/// The topic a heading names.
fn topic_named(title: &str) -> Option<Topic> {
    Topic::ALL
        .into_iter()
        .find(|topic| topic.name().eq_ignore_ascii_case(title.trim()))
}

/// The sections of the project guide that stand for a topic (the first of each name, outside any
/// other topic's section), and the headings that are neither a topic nor inside one.
fn split_sections(found: &[Section]) -> (Vec<(Topic, &Section)>, Vec<&Section>, Vec<&Section>) {
    let (mut topics, mut unknown, mut repeated) = (Vec::new(), Vec::new(), Vec::new());
    let mut covered_to = 0;
    for section in found {
        if section.start_line <= covered_to {
            continue;
        }
        match topic_named(&section.title) {
            Some(topic) => {
                covered_to = section.end_line;
                if topics.iter().any(|(t, _)| *t == topic) {
                    repeated.push(section);
                } else {
                    topics.push((topic, section));
                }
            }
            None => unknown.push(section),
        }
    }
    (topics, unknown, repeated)
}

/// The text of `topic` in the body of the project guide: its section as written, heading and
/// nested headings included, ending in one newline. `None` when the guide has no such section.
pub fn project_topic(body: &str, body_line: usize, topic: Topic) -> Option<String> {
    let found = sections(body, body_line);
    let (topics, _, _) = split_sections(&found);
    let (_, section) = topics.into_iter().find(|(t, _)| *t == topic)?;
    let mut text = section_text(body, body_line, section).trim_end().to_string();
    text.push('\n');
    Some(text)
}

impl Repo {
    /// The text of `topic` from the project's own `docs/instructions`; `None` when the store has no
    /// such doc, the doc cannot be read, or it has no section for the topic.
    pub fn project_guide(&self, topic: Topic) -> Option<String> {
        let files = self.store.stat_files().ok()?;
        let file = files.iter().find(|f| !f.plain && f.rel == "docs/instructions.md")?;
        let entry = self.store.read_entry(file).ok()?;
        let item = entry.item?;
        project_topic(&item.body, item.body_line, topic)
    }
}

/// The guide `stone instructions` prints from the directory `start`: the project's section when its
/// store has one for the topic, else the built-in text. Outside a store, or with a store that has
/// no such doc, it is the built-in text.
pub fn guide_text(start: &Path, topic: Topic) -> String {
    Repo::open(start)
        .ok()
        .and_then(|repo| repo.project_guide(topic))
        .unwrap_or_else(|| guide(topic).to_string())
}

/// Warnings for `docs/instructions`: a backticked `stone` command that stone does not have, a
/// heading that is not a topic, and a topic written twice.
pub(crate) fn guide_problems(snapshot: &Snapshot, out: &mut Vec<Problem>) {
    let Some(entry) = snapshot.entries.iter().find(|e| e.id.as_str() == PROJECT_DOC) else {
        return;
    };
    let Some(item) = &entry.item else { return };
    let path = entry.path.as_str();
    let found = sections(&item.body, item.body_line);
    let (_, unknown, repeated) = split_sections(&found);
    for section in unknown {
        out.push(Problem::warning(
            path,
            section.start_line,
            "guide",
            format!(
                "`{}` is not a guide topic; head the sections overview, plan, do, finish, debug or design",
                section.title
            ),
        ));
    }
    for section in repeated {
        out.push(Problem::warning(
            path,
            section.start_line,
            "guide",
            format!(
                "`{}` is written twice; only the first section is printed, so merge them",
                section.title
            ),
        ));
    }
    for (line, text, in_fence) in lines(&item.body, item.body_line) {
        if in_fence {
            continue;
        }
        for (from, to) in code_span_ranges(text) {
            let span = text[from..to].trim_matches('`').trim();
            if names_unknown_command(span) {
                out.push(Problem::warning(
                    path,
                    line,
                    "guide",
                    format!("`{span}` is not a stone command; `stone --help` lists the commands"),
                ));
            }
        }
    }
}

/// Whether a code span reads `stone <command> [<subcommand>]` with a command or subcommand stone
/// does not have. A span that is not a stone command line, or whose next word is a flag or a
/// placeholder, names nothing wrong.
fn names_unknown_command(span: &str) -> bool {
    let mut words = span.split_whitespace();
    if words.next() != Some("stone") {
        return false;
    }
    let Some(name) = words.next().filter(|w| !w.starts_with(['-', '<', '['])) else {
        return false;
    };
    if !CLI_COMMANDS.contains(&name) {
        return true;
    }
    CLI_NESTED.iter().any(|(parent, _)| *parent == name)
        && words
            .next()
            .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
            .is_some_and(|child| !CLI_NESTED.contains(&(name, child)))
}

/// The `stone` subcommands the overview may name; the CLI test checks the list
/// against its parser.
pub const CLI_COMMANDS: &[&str] = &[
    "check",
    "arch",
    "get",
    "list",
    "search",
    "code",
    "map",
    "scan",
    "init",
    "new",
    "edit",
    "link",
    "task",
    "config",
    "renumber",
    "rename",
    "delete",
    "mcp",
    "instructions",
    "why",
    "precedents",
    "history",
    "timeline",
    "commit-msg",
    "serve",
];

/// The nested subcommands as `(command, subcommand)`; the CLI test checks them
/// against its parser too.
pub const CLI_NESTED: &[(&str, &str)] = &[
    ("arch", "hld"),
    ("arch", "lld"),
    ("arch", "gaps"),
    ("arch", "map"),
    ("arch", "c4"),
    ("arch", "connect"),
    ("task", "move"),
    ("task", "next"),
    ("task", "fill-closed"),
    ("task", "remove"),
    ("config", "add-code-root"),
    ("config", "add-scip-index"),
];

/// Tools of other servers the guide may name: the Docs MCP.
pub const EXTERNAL_TOOLS: &[&str] = &["search_docs"];

/// Backticked words that are field names, not commands.
pub const FIELD_NAMES: &[&str] = &["libraries"];

const OVERVIEW: &str = include_str!("guide/overview.md");
const PLAN: &str = include_str!("guide/plan.md");
const DO: &str = include_str!("guide/do.md");
const FINISH: &str = include_str!("guide/finish.md");
const DEBUG: &str = include_str!("guide/debug.md");
const DESIGN: &str = include_str!("guide/design.md");
