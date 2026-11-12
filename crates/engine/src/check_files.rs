//! File-level rules of `stone check`: merge markers, `index.md` and `log.md`.

use carvestone_store::{Document, Store};

use crate::check::Problem;
use crate::index_file;
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
        for line in crate::merge::conflict_lines(text) {
            out.push(Problem::new(
                path,
                line,
                "conflict",
                format!(
                    "unresolved merge conflict: run `stone resolve {path} --take ours` or `--take theirs`, or edit the two versions into one"
                ),
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
            let stale = index_file::stale(text, snapshot);
            if !stale.is_empty() {
                out.push(Problem::warning(path, 1, "index", index_file::stale_message(&stale)));
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
