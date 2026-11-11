//! `stone config add-code-root` and `stone config add-scip-index`.
//!
//! Both edit the raw text of `stone/config.yml`, never a re-serialized copy, so its comments, its
//! other lines and its line ending stay. Config holds data only: no command is ever written here.

use carvestone_store::config::ScipIndexEntry;
use carvestone_store::write::write_atomic;
use carvestone_store::{Config, StoreError};
use serde_norway::Value;

use crate::create::Written;
use crate::repo::{EngineError, Repo};

const CONFIG_FILE: &str = "stone/config.yml";

/// The one entry to add, and the list it joins.
#[derive(Clone, Copy)]
enum Entry<'a> {
    Root(&'a str),
    Scip(&'a ScipIndexEntry),
}

impl Entry<'_> {
    fn key(self) -> &'static str {
        match self {
            Entry::Root(_) => "code_roots",
            Entry::Scip(_) => "scip_indexes",
        }
    }

    /// The config as it must read after the edit: the old one with this entry last in its list.
    fn added_to(self, old: &Config) -> Config {
        let mut config = old.clone();
        match self {
            Entry::Root(path) => config.code_roots.push(path.to_string()),
            Entry::Scip(entry) => config.scip_indexes.push(entry.clone()),
        }
        config
    }

    /// The lines of the entry in a block list whose dashes sit after `indent`.
    fn lines(self, indent: &str, eol: &str) -> Vec<String> {
        match self {
            Entry::Root(path) => vec![format!("{indent}- {}{eol}", scalar(path))],
            Entry::Scip(entry) => vec![
                format!("{indent}- file: {}{eol}", scalar(&entry.file)),
                format!("{indent}  language: {}{eol}", scalar(&entry.language)),
                format!(
                    "{indent}  extensions: [{}]{eol}",
                    entry
                        .extensions
                        .iter()
                        .map(|e| scalar(e))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ],
        }
    }
}

impl Repo {
    /// Adds `path` to `code_roots` in `stone/config.yml`. A folder already listed changes nothing.
    pub fn add_code_root(&self, path: &str, dry_run: bool) -> Result<Written, EngineError> {
        let path = path.trim();
        check_code_root(path)?;
        self.edit_config(dry_run, |old| {
            let config = Config::parse(old)?;
            if config.code_roots.iter().any(|root| root == path) {
                return Ok(None);
            }
            add_entry(old, &config, Entry::Root(path)).map(Some)
        })
    }

    /// Adds one entry to `scip_indexes` in `stone/config.yml`. The same entry again changes nothing;
    /// the same `file` with other fields is refused.
    pub fn add_scip_index(
        &self,
        file: &str,
        language: &str,
        extensions: &[String],
        dry_run: bool,
    ) -> Result<Written, EngineError> {
        let entry = ScipIndexEntry {
            file: file.trim().to_string(),
            language: language.trim().to_string(),
            extensions: extensions.iter().map(|e| e.trim().to_string()).collect(),
        };
        self.edit_config(dry_run, |old| {
            let config = Config::parse(old)?;
            if let Some(held) = config.scip_indexes.iter().find(|held| held.file == entry.file) {
                if *held == entry {
                    return Ok(None);
                }
                return Err(EngineError::Invalid(format!(
                    "`scip_indexes` already has an entry for `{}` (language `{}`, extensions `{}`); change that entry in {CONFIG_FILE} by hand, or name another file",
                    held.file,
                    held.language,
                    held.extensions.join(",")
                )));
            }
            add_entry(old, &config, Entry::Scip(&entry)).map(Some)
        })
    }

    /// Lock, re-read, let `change` compute the new text, save only if there is one. A dry run takes
    /// no lock and never saves.
    fn edit_config<F>(&self, dry_run: bool, change: F) -> Result<Written, EngineError>
    where
        F: FnOnce(&str) -> Result<Option<String>, EngineError>,
    {
        let lock = if dry_run { None } else { Some(self.store.lock()?) };
        let path = self.store.layout.data_dir.join("config.yml");
        let bytes = std::fs::read(&path).map_err(|source| {
            EngineError::from(StoreError::Io {
                path: path.display().to_string(),
                source,
            })
        })?;
        let old = String::from_utf8(bytes)
            .map_err(|_| EngineError::Invalid(format!("{CONFIG_FILE} is not valid UTF-8; nothing was written")))?;
        let new = change(&old)?;
        let changed = new.is_some();
        let text = new.unwrap_or(old);
        // The lock is held from the read above to here, so nobody saved between them.
        if changed && lock.is_some() {
            write_atomic(&path, &text)?;
        }
        Ok(Written {
            id: "config".into(),
            path: CONFIG_FILE.into(),
            changed,
            text,
            warnings: Vec::new(),
        })
    }
}

/// A code root is a folder inside the repo; `.` is the whole repo.
fn check_code_root(path: &str) -> Result<(), EngineError> {
    if path.is_empty() {
        return Err(EngineError::Invalid(
            "the code root is empty; name a folder inside the repo, such as `src` or `.`".into(),
        ));
    }
    let flat = path.replace('\\', "/");
    let drive = flat.as_bytes().get(1) == Some(&b':');
    if flat.starts_with('/') || drive || flat.split('/').any(|seg| seg == "..") {
        return Err(EngineError::Invalid(format!(
            "`{path}` is outside the repo; name a folder inside the repo, relative to its root, such as `src` or `.`"
        )));
    }
    Ok(())
}

/// The text of `old` with `entry` added to its list, checked to read back as exactly that.
fn add_entry(old: &str, config: &Config, entry: Entry<'_>) -> Result<String, EngineError> {
    let key = entry.key();
    let eol = if old.find('\n').is_some_and(|n| old[..n].ends_with('\r')) {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = old.split_inclusive('\n').map(str::to_string).collect();
    let (at, indent) = match find_shape(&lines, key)? {
        Shape::Missing => {
            let at = lines.len();
            insert(&mut lines, at, vec![format!("{key}:{eol}")], eol);
            (at + 1, "  ".to_string())
        }
        Shape::Block { at, indent } => (at, indent),
        Shape::Flow { line, open, close } => {
            let body = body_of(&lines[line]).to_string();
            let end = terminator_of(&lines[line]).to_string();
            match entry {
                Entry::Root(path) => {
                    // The new item goes in front of the closing bracket; the rest of the line stays.
                    let head = body[..close].trim_end();
                    let sep = if head.ends_with('[') { "" } else { ", " };
                    lines[line] = format!("{head}{sep}{}{}{end}", scalar(path), &body[close..]);
                    return finish(config, entry, lines);
                }
                Entry::Scip(_) => {
                    // An empty `scip_indexes: []` becomes the key alone, and the entry follows as a block.
                    if !body[open + 1..close].trim().is_empty() {
                        return Err(not_a_list(key));
                    }
                    lines[line] = format!("{}{}{end}", body[..open].trim_end(), &body[close + 1..]);
                    (line + 1, "  ".to_string())
                }
            }
        }
    };
    insert(&mut lines, at, entry.lines(&indent, eol), eol);
    finish(config, entry, lines)
}

/// Joins the lines and checks the result: it has to parse, and read as the old config plus the entry.
fn finish(config: &Config, entry: Entry, lines: Vec<String>) -> Result<String, EngineError> {
    let new = lines.concat();
    let parsed = Config::parse(&new)?;
    if parsed != entry.added_to(config) {
        return Err(EngineError::Invalid(format!(
            "the new `{}` entry would not read back as written; check the value, or edit {CONFIG_FILE} by hand",
            entry.key()
        )));
    }
    Ok(new)
}

/// Puts `new` at line `at`. A last line with no line ending gets one first.
fn insert(lines: &mut Vec<String>, at: usize, new: Vec<String>, eol: &str) {
    if at > 0
        && let Some(before) = lines.get_mut(at - 1)
        && !before.ends_with('\n')
    {
        before.push_str(eol);
    }
    lines.splice(at..at, new);
}

/// How a key stands in the file.
enum Shape {
    /// No top-level line for the key.
    Missing,
    /// `key:` with its items on the lines below; `at` is the line after the last item.
    Block { at: usize, indent: String },
    /// `key: [...]` on one line; `open` and `close` are the byte offsets of the brackets in it.
    Flow { line: usize, open: usize, close: usize },
}

fn find_shape(lines: &[String], key: &str) -> Result<Shape, EngineError> {
    let Some(index) = lines.iter().position(|line| {
        body_of(line)
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    }) else {
        return Ok(Shape::Missing);
    };
    let body = body_of(&lines[index]);
    let after = &body[key.len() + 1..];
    let value = after.trim_start();
    if value.is_empty() || value.starts_with('#') {
        return block(lines, index, key);
    }
    if value.starts_with('[') {
        let open = body.len() - value.len();
        let close = open + flow_close(value).ok_or_else(|| not_a_list(key))?;
        let tail = body[close + 1..].trim_start();
        if !tail.is_empty() && !tail.starts_with('#') {
            return Err(not_a_list(key));
        }
        return Ok(Shape::Flow {
            line: index,
            open,
            close,
        });
    }
    Err(not_a_list(key))
}

/// The block below line `index`: every indented line (or dash line) up to the next top-level key.
fn block(lines: &[String], index: usize, key: &str) -> Result<Shape, EngineError> {
    let mut first = None;
    let mut last = None;
    for (n, line) in lines.iter().enumerate().skip(index + 1) {
        let body = body_of(line);
        let text = body.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        if body.starts_with([' ', '\t']) || body == "-" || body.starts_with("- ") {
            first.get_or_insert(n);
            last = Some(n);
        } else {
            break;
        }
    }
    let Some(first) = first else {
        return Ok(Shape::Block {
            at: index + 1,
            indent: "  ".to_string(),
        });
    };
    let head = body_of(&lines[first]);
    if !head.trim_start().starts_with('-') {
        return Err(not_a_list(key));
    }
    Ok(Shape::Block {
        at: last.map_or(index + 1, |n| n + 1),
        indent: head[..head.len() - head.trim_start().len()].to_string(),
    })
}

/// The offset of the `]` that closes the `[` at the start of `value`, skipping quoted text.
fn flow_close(value: &str) -> Option<usize> {
    let mut chars = value.char_indices().skip(1).peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            ']' => return Some(at),
            '\'' => {
                while let Some((_, q)) = chars.next() {
                    if q == '\'' {
                        if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            '"' => {
                while let Some((_, q)) = chars.next() {
                    match q {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn not_a_list(key: &str) -> EngineError {
    EngineError::Invalid(format!(
        "`{key}` in {CONFIG_FILE} is not a list stone can extend; add the entry to it by hand"
    ))
}

/// The line without its line ending.
fn body_of(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

fn terminator_of(line: &str) -> &str {
    &line[body_of(line).len()..]
}

/// `text` as a YAML scalar: plain when that reads back as the same string, otherwise single-quoted.
fn scalar(text: &str) -> String {
    let plain = (text.starts_with(|c: char| c.is_ascii_alphanumeric()) || (text.starts_with('.') && text.len() > 1))
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        && matches!(serde_norway::from_str::<Value>(text), Ok(Value::String(s)) if s == text);
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "''"))
    }
}
