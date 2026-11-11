//! File-level rules of `stone check`: merge markers, `index.md` and `log.md`.

use carvestone_store::{Document, Store};

use crate::check::Problem;
use crate::repo::EngineError;
use crate::resolve::Snapshot;

const ROOT_INDEX: &str = "stone/index.md";

pub(crate) fn file_problems(store: &Store, snapshot: &Snapshot, out: &mut Vec<Problem>) -> Result<(), EngineError> {
    let reserved = store.scan_reserved()?;
    let texts = snapshot
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.text.as_str()))
        .chain(reserved.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    for (path, text) in texts {
        if let Some(line) = marker_line(text) {
            out.push(Problem::new(
                path,
                line,
                "conflict",
                "unresolved merge conflict; resolve it and remove the markers",
            ));
        }
    }

    let mut has_root_index = false;
    for (path, text) in &reserved {
        if path == ROOT_INDEX {
            has_root_index = true;
            let declares_version = Document::parse(text).is_ok_and(|d| d.fields().contains_key("okf_version"));
            if !declares_version {
                out.push(Problem::new(
                    path,
                    1,
                    "index",
                    "the root index must start with frontmatter declaring `okf_version`",
                ));
            }
        } else if text.starts_with("---\n") || text.starts_with("---\r\n") {
            out.push(Problem::new(
                path,
                1,
                "index",
                "index.md and log.md below the root carry no frontmatter",
            ));
        }
    }
    if !has_root_index {
        out.push(Problem::new(
            ROOT_INDEX,
            1,
            "index",
            "stone/index.md is missing; `stone init` creates it",
        ));
    }
    Ok(())
}

/// The 1-based line of the first merge marker: exactly seven `<` or `>`
/// followed by a space or the end of the line.
fn marker_line(text: &str) -> Option<usize> {
    text.lines()
        .position(|line| {
            ["<<<<<<<", ">>>>>>>"].iter().any(|m| {
                line.strip_prefix(m)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
            })
        })
        .map(|i| i + 1)
}
