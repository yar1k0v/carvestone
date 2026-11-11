//! The lines an empty code answer carries: what Stone indexes for the target, what it never
//! indexes, and the search to run next.

use rusqlite::{Connection, OptionalExtension};

use crate::code::Target;
use crate::repo::EngineError;

/// What an empty answer is about, which decides the first line.
pub(crate) enum Asked<'a> {
    /// A target Stone parses, in this language.
    Lang(&'a str),
    /// A file in a type Stone does not parse.
    UnparsedFile,
    /// No target (`find`): everything the store holds.
    Store,
}

/// The lines, joined with `\n`, no newline at the end.
pub(crate) fn gap(conn: &Connection, asked: Asked, terms: &[&str]) -> Result<String, EngineError> {
    let first = match asked {
        Asked::Lang(lang) => {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT e.kind FROM edge_ends e JOIN files f ON f.id = e.file_id \
                 WHERE f.lang = ?1 ORDER BY e.kind",
            )?;
            let kinds: Vec<String> = stmt.query_map([lang], |row| row.get(0))?.collect::<Result<_, _>>()?;
            if kinds.is_empty() {
                format!("indexed for {lang}: declarations only, no links between them yet")
            } else {
                format!(
                    "indexed for {lang}: declarations, and these links between them: {}",
                    kinds.join(", ")
                )
            }
        }
        Asked::UnparsedFile => "indexed for this file: nothing, Stone does not parse this file type".to_string(),
        Asked::Store => {
            // A project manifest (`Cargo.toml`, `*.csproj`) is a row too, but not a language
            // Stone parses for declarations, so it is left out here as `code native` does.
            let mut stmt = conn.prepare(
                "SELECT DISTINCT lang FROM files WHERE lang IS NOT NULL AND lang <> 'project' ORDER BY lang",
            )?;
            let langs: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
            if langs.is_empty() {
                "indexed here: no code".to_string()
            } else {
                format!("indexed here: declarations of {}", langs.join(", "))
            }
        }
    };
    // Every term is quoted for the shell, so a name with a quote cannot break the command.
    let search: String = terms
        .iter()
        .map(|term| format!(" -e \"{}\"", term.replace('"', "")))
        .collect();
    Ok(format!(
        "{first}\nnot indexed: string literals, config keys\nnext: git grep -n -F{search}"
    ))
}

/// The part of a qualified name or file path that appears in the source as text: a qualified
/// name is not written out in full, and a file is named by its stem.
pub(crate) fn term(label: &str, is_file: bool) -> &str {
    if is_file {
        let name = label.rsplit('/').next().unwrap_or(label);
        name.rsplit_once('.').map_or(name, |(stem, _)| stem)
    } else {
        let colon = label.rfind("::").map(|at| at + 2);
        let dot = label.rfind('.').map(|at| at + 1);
        colon.max(dot).map_or(label, |at| &label[at..])
    }
}

/// The language Stone stores for the target's file; `None` for a file it does not parse.
fn lang_of(conn: &Connection, target: &Target) -> Result<Option<String>, EngineError> {
    let (sql, id) = match target.file_id {
        Some(id) => ("SELECT lang FROM files WHERE id = ?1", id),
        None => {
            let Some(id) = target.ids.first() else {
                return Ok(None);
            };
            (
                "SELECT f.lang FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.id = ?1",
                *id,
            )
        }
    };
    // A NULL lang is a row that is there but unparsed, so it is not the same as no row.
    let lang: Option<Option<String>> = conn.query_row(sql, [id], |row| row.get(0)).optional()?;
    Ok(lang.flatten())
}

fn with_lang(conn: &Connection, lang: Option<&str>, terms: &[&str]) -> Result<String, EngineError> {
    let asked = match lang {
        Some(lang) => Asked::Lang(lang),
        None => Asked::UnparsedFile,
    };
    gap(conn, asked, terms)
}

/// The lines for one target: its language, and its own term.
pub(crate) fn of_target(conn: &Connection, target: &Target) -> Result<String, EngineError> {
    let lang = lang_of(conn, target)?;
    let term = term(&target.label, target.file_id.is_some());
    with_lang(conn, lang.as_deref(), &[term])
}

/// The lines for a `path` with one end: both terms, the first target's language.
pub(crate) fn of_targets(conn: &Connection, a: &Target, b: &Target) -> Result<String, EngineError> {
    let lang = lang_of(conn, a)?;
    let terms = [term(&a.label, a.file_id.is_some()), term(&b.label, b.file_id.is_some())];
    with_lang(conn, lang.as_deref(), &terms)
}
