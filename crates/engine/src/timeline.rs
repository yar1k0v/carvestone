//! `stone timeline`: commits, completed tasks, accepted decisions and doc
//! edits merged into one list, newest first.
//!
//! Commits come from the git cache. A task counts as done by the commit that
//! added its `status: done` line, a decision as accepted by the commit that
//! added `status: accepted`; a task reopened and finished again gives two events.

use std::time::{SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::git_cache::ensure_filled;
use crate::owner::arch_elements;
use crate::refresh::refresh;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::why::names;

const DEFAULT_LIMIT: usize = 30;
const TASK_DONE: &str = "status: done";
const DECISION_ACCEPTED: &str = "status: accepted";

/// A calendar day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// `None` for a day that does not exist.
    pub fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Today in UTC.
    pub fn today() -> Date {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Date::from_days(secs.div_euclid(86_400))
    }

    /// The UTC day of a Unix time in seconds.
    pub fn from_unix(secs: i64) -> Date {
        Date::from_days(secs.div_euclid(86_400))
    }

    pub fn minus_days(self, days: i64) -> Date {
        Date::from_days(self.days() - days)
    }

    /// Days since Monday: 0 for a Monday, 6 for a Sunday.
    pub fn weekday(self) -> i64 {
        (self.days() + 3).rem_euclid(7)
    }

    /// `YYYY-MM-DD`.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Days since 1970-01-01.
    fn days(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn from_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = (yoe + era * 400) as i32 + i32::from(month <= 2);
        Date { year, month, day }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        _ => 28,
    }
}

/// `YYYY-MM-DD`, or `Nd` for N days before `today`. `flag` names the option in the error.
pub fn parse_date(flag: &str, text: &str, today: Date) -> Result<Date, EngineError> {
    let bad = || EngineError::Invalid(format!("{flag} must be YYYY-MM-DD or Nd"));
    let text = text.trim();
    if let Some(days) = text.strip_suffix('d')
        && !days.is_empty()
        && days.bytes().all(|b| b.is_ascii_digit())
    {
        let days: i64 = days.parse().map_err(|_| bad())?;
        return Ok(today.minus_days(days));
    }
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return Err(bad());
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(bad());
    }
    let (Ok(y), Ok(m), Ok(d)) = (y.parse(), m.parse(), d.parse()) else {
        return Err(bad());
    };
    Date::new(y, m, d).ok_or_else(bad)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Commit,
    TaskDone,
    Decision,
    Doc,
}

impl EventKind {
    pub const NAMES: &'static str = "commit, task_done, decision, doc";

    pub fn parse(name: &str) -> Result<EventKind, EngineError> {
        match name {
            "commit" => Ok(EventKind::Commit),
            "task_done" => Ok(EventKind::TaskDone),
            "decision" => Ok(EventKind::Decision),
            "doc" => Ok(EventKind::Doc),
            other => Err(EngineError::Invalid(format!(
                "unknown kind `{other}`; one of {}",
                EventKind::NAMES
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EventKind::Commit => "commit",
            EventKind::TaskDone => "task_done",
            EventKind::Decision => "decision",
            EventKind::Doc => "doc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Event {
    /// The commit's author date, ISO 8601.
    pub at: String,
    pub kind: EventKind,
    /// The short sha of a commit, else the item's id.
    pub id: String,
    /// The commit subject, else the item's title.
    pub title: String,
    pub sha: String,
}

impl Event {
    pub fn line(&self) -> String {
        let day = self.at.get(..10).unwrap_or(&self.at);
        let mut line = format!("{day}  {}  {}  {}", self.kind.name(), self.id, self.title);
        if self.kind != EventKind::Commit {
            line.push_str(&format!("  ({})", self.sha.get(..7).unwrap_or(&self.sha)));
        }
        line.trim_end().to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TimelineOpts {
    pub since: Option<Date>,
    pub until: Option<Date>,
    pub component: Option<String>,
    /// Empty means every kind.
    pub kinds: Vec<EventKind>,
    /// Zero means 30.
    pub limit: usize,
}

struct Ranked {
    ts: i64,
    event: Event,
}

/// What `--component` keeps: commits under its code globs, items that list it.
struct Component {
    id: String,
    globs: Option<GlobSet>,
}

impl Component {
    fn load(snapshot: &Snapshot, input: &str) -> Result<Component, EngineError> {
        let (entry, _) = snapshot.resolve(input)?;
        let id = entry.id.as_str().to_string();
        let mut problems = Vec::new();
        let globs = arch_elements(snapshot, &mut problems)
            .into_iter()
            .find(|e| e.id == id)
            .map(|e| e.globs);
        Ok(Component { id, globs })
    }

    fn holds_commit(&self, conn: &Connection, repo: &str, sha: &str) -> Result<bool, EngineError> {
        let Some(globs) = &self.globs else {
            return Ok(false);
        };
        let mut stmt = conn.prepare_cached("SELECT path FROM commit_files WHERE repo = ?1 AND sha = ?2")?;
        let paths = stmt.query_map(params![repo, sha], |row| row.get::<_, String>(0))?;
        for path in paths {
            if globs.is_match(path?) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn lists(&self, snapshot: &Snapshot, path: &str) -> bool {
        snapshot
            .entries
            .iter()
            .find(|e| e.path == path)
            .and_then(|e| e.item.as_ref())
            .is_some_and(|item| names(snapshot, &item.fields.components, &self.id))
    }
}

pub fn timeline(repo: &Repo, opts: &TimelineOpts) -> Result<Vec<Event>, EngineError> {
    refresh(repo)?;
    ensure_filled(repo)?;
    let snapshot = repo.snapshot()?;
    let component = match &opts.component {
        Some(input) => Some(Component::load(&snapshot, input)?),
        None => None,
    };
    let index = repo.index()?;
    let conn = index.conn();
    let limit = if opts.limit == 0 { DEFAULT_LIMIT } else { opts.limit };
    let want = |kind: EventKind| opts.kinds.is_empty() || opts.kinds.contains(&kind);
    let since = opts.since.map(Date::iso);
    let until = opts.until.map(Date::iso);
    let in_range = |at: &str| {
        let day = at.get(..10).unwrap_or(at);
        since.as_deref().is_none_or(|s| day >= s) && until.as_deref().is_none_or(|u| day <= u)
    };

    let mut found: Vec<Ranked> = Vec::new();

    if want(EventKind::Commit) {
        let mut stmt = conn.prepare(
            "SELECT sha, date, ts, subject, repo FROM commits \
             WHERE (?1 IS NULL OR substr(date, 1, 10) >= ?1) AND (?2 IS NULL OR substr(date, 1, 10) <= ?2) \
             ORDER BY ts DESC, sha ASC, repo ASC",
        )?;
        let mut rows = stmt.query(params![since, until])?;
        let mut kept = 0;
        while kept < limit
            && let Some(row) = rows.next()?
        {
            let sha: String = row.get(0)?;
            let from: String = row.get(4)?;
            if let Some(c) = &component
                && !c.holds_commit(conn, &from, &sha)?
            {
                continue;
            }
            found.push(Ranked {
                ts: row.get(2)?,
                event: Event {
                    at: row.get(1)?,
                    kind: EventKind::Commit,
                    id: sha.get(..7).unwrap_or(&sha).to_string(),
                    title: row.get(3)?,
                    sha,
                },
            });
            kept += 1;
        }
    }

    if want(EventKind::Doc) {
        let mut stmt = conn.prepare(
            "SELECT c.sha, c.date, c.ts, f.path, c.repo FROM commit_files f \
             JOIN commits c ON c.repo = f.repo AND c.sha = f.sha \
             WHERE f.path LIKE 'stone/docs/%' \
             AND (?1 IS NULL OR substr(c.date, 1, 10) >= ?1) AND (?2 IS NULL OR substr(c.date, 1, 10) <= ?2) \
             ORDER BY c.ts DESC, c.sha ASC, f.path ASC",
        )?;
        let mut rows = stmt.query(params![since, until])?;
        let mut kept = 0;
        while kept < limit
            && let Some(row) = rows.next()?
        {
            let (sha, path, from): (String, String, String) = (row.get(0)?, row.get(3)?, row.get(4)?);
            if let Some(c) = &component
                && !(c.lists(&snapshot, &path) || c.holds_commit(conn, &from, &sha)?)
            {
                continue;
            }
            let (id, title) = item_of(&snapshot, &path);
            found.push(Ranked {
                ts: row.get(2)?,
                event: Event {
                    at: row.get(1)?,
                    kind: EventKind::Doc,
                    id,
                    title,
                    sha,
                },
            });
            kept += 1;
        }
    }

    if want(EventKind::TaskDone) || want(EventKind::Decision) {
        // Items live in the repo that holds `stone/`; its paths map back to root-relative ones.
        let repos = repo.repos();
        let mut added = Vec::new();
        if let Some(stone) = repos.stone_repo() {
            let inner: Vec<String> = ["stone/tasks", "stone/decisions"]
                .iter()
                .filter_map(|p| repos.repo_of(p))
                .filter(|(owner, _)| owner.dir() == stone.dir())
                .map(|(_, inner)| inner)
                .collect();
            let paths: Vec<&str> = inner.iter().map(String::as_str).collect();
            added = stone
                .git()
                .added_lines("^status: (done|accepted)[[:space:]]*$", "status: ", &paths)?;
            for line in &mut added {
                line.path = repos.to_root(stone, &line.path);
            }
        }
        let from = repos.stone_repo().map(|r| r.dir().to_string()).unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        for line in added {
            let kind = match (line.text.trim_end(), folder_of(&line.path)) {
                (TASK_DONE, "tasks") => EventKind::TaskDone,
                (DECISION_ACCEPTED, "decisions") => EventKind::Decision,
                _ => continue,
            };
            if !want(kind) || !seen.insert((line.sha.clone(), line.path.clone())) {
                continue;
            }
            let at: Option<String> = conn
                .query_row(
                    "SELECT date FROM commits WHERE repo = ?1 AND sha = ?2",
                    params![from, line.sha],
                    |row| row.get(0),
                )
                .ok();
            let Some(at) = at.filter(|at| in_range(at)) else {
                continue;
            };
            if let Some(c) = &component
                && !(c.lists(&snapshot, &line.path) || c.holds_commit(conn, &from, &line.sha)?)
            {
                continue;
            }
            let (id, title) = item_of(&snapshot, &line.path);
            found.push(Ranked {
                ts: line.timestamp,
                event: Event {
                    at,
                    kind,
                    id,
                    title,
                    sha: line.sha,
                },
            });
        }
    }

    found.sort_by(|a, b| {
        b.ts.cmp(&a.ts)
            .then_with(|| a.event.sha.cmp(&b.event.sha))
            .then_with(|| a.event.kind.cmp(&b.event.kind))
            .then_with(|| a.event.id.cmp(&b.event.id))
    });
    found.truncate(limit);
    Ok(found.into_iter().map(|r| r.event).collect())
}

/// The folder under `stone/` a path sits in.
fn folder_of(path: &str) -> &str {
    path.strip_prefix("stone/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
}

/// The id and title of the item at `path`; a file that is gone is named by its file name.
fn item_of(snapshot: &Snapshot, path: &str) -> (String, String) {
    match snapshot.entries.iter().find(|e| e.path == path) {
        Some(entry) => (
            snapshot.label(entry),
            entry
                .item
                .as_ref()
                .and_then(|i| i.fields.title.clone())
                .unwrap_or_default(),
        ),
        None => {
            let name = path.rsplit('/').next().unwrap_or(path);
            (name.strip_suffix(".md").unwrap_or(name).to_string(), String::new())
        }
    }
}
