//! Creating items: `init` and `new`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use carvestone_core::Actor;
use carvestone_store::write::write_atomic;
use carvestone_store::{Config, StoreError};
use serde::Serialize;
use serde_norway::{Mapping, Value};

use crate::code_native::{self, LangSetup};
use crate::fields::SetValue;
use crate::repo::{EngineError, Repo};

/// What a write command did. `text` is the whole file as it is on disk, or as it would be after a dry run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Written {
    pub id: String,
    pub path: String,
    pub changed: bool,
    pub text: String,
    pub warnings: Vec<String>,
}

/// A request for `stone new`. `set` holds the fields to change; `sections` are names with their text.
#[derive(Debug, Clone, Default)]
pub struct NewItem {
    pub type_name: String,
    pub title: String,
    pub slug: Option<String>,
    pub set: Vec<(String, SetValue)>,
    pub sections: Vec<(String, String)>,
    /// The whole body, kept as written; only for types whose template is empty (docs, plans, refs).
    pub body: Option<String>,
    /// Link targets for `links`, each as written; one that names nothing yet is not an error.
    pub links: Vec<String>,
    /// Front matter fields of a source file that the new item does not keep.
    pub left_out: Vec<String>,
}

/// The agent override wins, then the git user, then `human:unknown`.
fn author_from(agent: Option<&str>, git_name: Option<&str>) -> String {
    if let Some(agent) = agent.map(str::trim).filter(|a| !a.is_empty()) {
        return agent.to_string();
    }
    let slug: String = git_name
        .unwrap_or_default()
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "human:unknown".to_string()
    } else {
        format!("human:{slug}")
    }
}

/// Who is writing: `STONE_AGENT`, else Claude Code when it runs the shell, else the git user configured for the repo at `root`, wherever the process runs.
pub fn author_in(root: &Path) -> String {
    let agent = std::env::var("STONE_AGENT").ok();
    if agent.as_deref().is_some_and(|a| !a.trim().is_empty()) {
        return author_from(agent.as_deref(), None);
    }
    // Every agent shell starts fresh, so an agent's writes would otherwise carry the human's git name.
    if std::env::var("CLAUDECODE").is_ok_and(|v| v == "1") {
        return "claude-code".to_string();
    }
    let git = Command::new("git")
        .current_dir(root)
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    author_from(None, git.as_deref())
}

/// The repo's own actor, else the git user of the repo that holds `stone/` (the product root
/// when that is in no repo), checked as an actor: a bad name stops the write instead of writing
/// a stamp `stone check` rejects.
pub(crate) fn stamp_by(repo: &Repo) -> Result<String, EngineError> {
    let repos = repo.repos();
    let dir = repos
        .stone_repo()
        .map_or(repo.store.layout.root.as_path(), |r| r.git().dir());
    let by = repo.author().map_or_else(|| author_in(dir), str::to_string);
    match by.parse::<Actor>() {
        Ok(_) => Ok(by),
        Err(e) => Err(EngineError::Invalid(format!(
            "cannot stamp as `{by}`: {e}; set STONE_AGENT to `human:<id>`, `process:<id>` or a producer name"
        ))),
    }
}

/// The `{ by, at }` map of a `generated` or `verified` entry.
pub(crate) fn stamp_value(by: &str, at: &str) -> Value {
    let mut map = Mapping::new();
    map.insert(Value::String("by".into()), Value::String(by.into()));
    map.insert(Value::String("at".into()), Value::String(at.into()));
    Value::Mapping(map)
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn date_of(secs: u64) -> String {
    let (year, month, day) = civil((secs / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A Unix time as `YYYY-MM-DDThh:mm:ss+00:00`, in UTC.
pub fn timestamp_of(secs: u64) -> String {
    let rest = secs % 86_400;
    format!(
        "{}T{:02}:{:02}:{:02}+00:00",
        date_of(secs),
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Today's UTC date as `YYYY-MM-DD`.
pub fn today() -> String {
    date_of(unix_now())
}

/// The UTC time now, in the form new writes use.
pub fn now() -> String {
    timestamp_of(unix_now())
}

const FOLDERS: [&str; 7] = ["tasks", "milestones", "decisions", "docs", "plans", "refs", "arch"];
pub(crate) const INDEX: &str = "---\nokf_version: \"0.2\"\n---\n";

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// What happened to git for `stone/` during `init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoneRepo {
    /// The root is in a repo, or `stone/` already is one: nothing to do.
    NotNeeded,
    /// `git init` ran in `stone/`.
    Created,
    /// `stone/` is in no repo and `git init` could not run; the text says why.
    Skipped(String),
}

/// What `init` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initialised {
    pub config: PathBuf,
    /// The project repos written to `code_roots`, with a trailing `/`; just `.` when the root is itself in a repo.
    pub code_roots: Vec<String>,
    pub stone_repo: StoneRepo,
    /// Lines added to the root `.gitignore`.
    pub ignored: Vec<String>,
    /// The languages found under the code roots, for the next steps `init` prints.
    pub langs: Vec<LangSetup>,
}

/// Direct subfolders of `root` that hold a `.git` (folder or file), sorted, as `name/`.
fn project_repos(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.path().join(".git").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "stone")
        .collect();
    found.sort();
    found.into_iter().map(|name| format!("{name}/")).collect()
}

/// A YAML list item for a folder name, quoted only when plain YAML could misread it.
fn yaml_item(text: &str) -> String {
    let plain = text.starts_with(|c: char| c.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if plain {
        format!("  - {text}\n")
    } else {
        format!("  - '{}'\n", text.replace('\'', "''"))
    }
}

/// Runs `git init` in `data` unless it is already inside a repo.
fn init_stone_repo(data: &Path) -> StoneRepo {
    if let Err(e) = carvestone_git::check_installed() {
        return StoneRepo::Skipped(e.to_string());
    }
    match Command::new("git").current_dir(data).args(["init", "-q"]).output() {
        Ok(out) if out.status.success() => StoneRepo::Created,
        Ok(out) => StoneRepo::Skipped(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => StoneRepo::Skipped(e.to_string()),
    }
}

/// Creates `stone/` under `root`. In a folder that is in no git repo it lists the project repos
/// below it as `code_roots` and makes `stone/` a repo of its own. In a git repo it sets `code_roots`
/// to `.`, the whole repo, so its code is indexed.
pub fn init(root: &Path, prefix: Option<&str>) -> Result<Initialised, EngineError> {
    init_with(root, prefix, false)
}

/// `init`, and with `done_needs_plan` true the config also turns on the rule that a task needs a
/// plan and an outcome section before it can move to done.
pub fn init_with(root: &Path, prefix: Option<&str>, done_needs_plan: bool) -> Result<Initialised, EngineError> {
    let root = std::path::absolute(root).map_err(io_err(root))?;
    let data = root.join("stone");
    let config_path = data.join("config.yml");
    if config_path.exists() {
        return Err(EngineError::Invalid("already initialised".into()));
    }
    let prefix = prefix.unwrap_or("TASK");
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let in_repo = crate::repos::inside_repo(&root);
    let code_roots = if in_repo {
        vec![".".to_string()]
    } else {
        project_repos(&root)
    };
    let mut config = format!("project: '{}'\ntask_prefix: {prefix}\n", name.replace('\'', "''"));
    if !code_roots.is_empty() {
        config.push_str("code_roots:\n");
        for dir in &code_roots {
            config.push_str(&yaml_item(dir));
        }
    }
    if done_needs_plan {
        config.push_str("done_needs_plan: true\n");
    }
    // `Config::parse` is the one judge of a prefix; the comparison catches values YAML reads as a number or null.
    if Config::parse(&config)?.task_prefix != prefix {
        return Err(EngineError::Invalid(format!(
            "invalid prefix {prefix:?}: use ASCII letters and digits, starting with a letter"
        )));
    }
    for folder in FOLDERS {
        let dir = data.join(folder);
        std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    }
    let index = data.join("index.md");
    if !index.exists() {
        write_atomic(&index, INDEX)?;
    }
    // The ignore file is only a repo's business when the root is in that repo; a plain product folder has none to touch.
    let mut ignored = Vec::new();
    if in_repo {
        let mut wanted = vec!["/.stone/"];
        // The binary dropped next to the project is not source.
        if root.join("stone.exe").is_file() {
            wanted.push("stone.exe");
        }
        ignored = ignore_lines(&root, &wanted)?;
    }
    // Last, so a failed init leaves no config and can be run again.
    write_atomic(&config_path, &config)?;
    let stone_repo = if in_repo || data.join(".git").exists() {
        StoneRepo::NotNeeded
    } else {
        init_stone_repo(&data)
    };
    // Only a hint: a store that cannot be opened here still initialised.
    let langs = Repo::open(&root)
        .map(|repo| code_native::setup(&repo, None))
        .unwrap_or_default();
    Ok(Initialised {
        config: config_path,
        code_roots,
        stone_repo,
        ignored,
        langs,
    })
}

/// Adds each of `wanted` to the root `.gitignore` unless a line already ignores it (a leading or
/// trailing `/` does not matter); keeps the file's line ending. Returns the lines added.
fn ignore_lines(root: &Path, wanted: &[&str]) -> Result<Vec<String>, StoreError> {
    let path = root.join(".gitignore");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(io_err(&path)(e)),
    };
    let bare = |line: &str| line.trim().trim_matches('/').to_string();
    let present: Vec<String> = text.lines().map(bare).collect();
    let added: Vec<String> = wanted
        .iter()
        .filter(|w| !present.contains(&bare(w)))
        .map(|w| w.to_string())
        .collect();
    if added.is_empty() {
        return Ok(added);
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = text;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    for line in &added {
        out.push_str(line);
        out.push_str(eol);
    }
    write_atomic(&path, &out)?;
    Ok(added)
}
