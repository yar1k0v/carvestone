//! Work done by day, week or month: the tasks finished, the docs and decisions changed and how
//! many commits landed. Unlike the timeline it has no row limit, so any period reads whole.

use std::collections::BTreeMap;

use carvestone_core::{Item, ItemType};
use carvestone_store::Entry;
use rusqlite::params;
use serde::Serialize;

use crate::docs_list::uncommitted;
use crate::git_cache::index_with_history;
use crate::repo::{EngineError, Repo};
use crate::resolve::Snapshot;
use crate::stale::day_of;
use crate::timeline::Date;
use crate::work_links::{WorkLinks, tasks};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
/// Periods shown when the caller names no start.
const DEFAULT_DAYS: i64 = 14;
const DEFAULT_WEEKS: i64 = 8;
const DEFAULT_MONTHS: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
}

impl Period {
    pub fn parse(name: &str) -> Result<Period, EngineError> {
        match name {
            "day" => Ok(Period::Day),
            "week" => Ok(Period::Week),
            "month" => Ok(Period::Month),
            other => Err(EngineError::Invalid(format!(
                "unknown period `{other}`; one of day, week, month"
            ))),
        }
    }

    /// The first day of the period `day` falls in. A week starts on Monday.
    fn start(self, day: Date) -> Date {
        match self {
            Period::Day => day,
            Period::Week => day.minus_days(day.weekday()),
            Period::Month => day.minus_days(i64::from(day.day) - 1),
        }
    }

    fn label(self, start: Date) -> String {
        let month = MONTHS[start.month as usize - 1];
        match self {
            Period::Day => format!("{} {month} {}", start.day, start.year),
            Period::Week => format!("Week of {} {month} {}", start.day, start.year),
            Period::Month => format!("{month} {}", start.year),
        }
    }

    /// The first day shown when the caller names none.
    fn default_from(self, today: Date) -> Date {
        match self {
            Period::Day => today.minus_days(DEFAULT_DAYS - 1),
            Period::Week => today.minus_days((DEFAULT_WEEKS - 1) * 7),
            Period::Month => {
                let months = today.year * 12 + today.month as i32 - 1 - (DEFAULT_MONTHS - 1);
                Date::new(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1).unwrap_or(today)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActivityOpts {
    pub period: Period,
    /// The oldest day to show, moved back to the start of its period; `None` is a recent stretch.
    pub from: Option<Date>,
    /// The newest day to show; `None` is no limit.
    pub to: Option<Date>,
}

/// A task finished in a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DoneTask {
    pub id: String,
    pub title: String,
    /// `YYYY-MM-DD`.
    pub closed: String,
    /// The task records no closed day; this one comes from its commits or its file.
    pub estimated: bool,
}

/// A doc or decision changed in a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChangedItem {
    pub id: String,
    pub title: String,
    /// `YYYY-MM-DD` of its newest change in the period.
    pub day: String,
}

/// What happened in one day, week or month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActivityGroup {
    pub label: String,
    /// `YYYY-MM-DD` of the period's first day.
    pub start: String,
    pub tasks_done: Vec<DoneTask>,
    pub docs_changed: Vec<ChangedItem>,
    pub decisions_changed: Vec<ChangedItem>,
    pub commit_count: u32,
}

/// The groups being filled, by the first day of their period.
struct Groups {
    opts: ActivityOpts,
    from: Date,
    by_start: BTreeMap<Date, ActivityGroup>,
}

impl Groups {
    /// The group of `day`, when the day is inside the asked range.
    fn at(&mut self, text: &str) -> Option<&mut ActivityGroup> {
        let day = day_of(text)?;
        if day < self.from || self.opts.to.is_some_and(|to| day > to) {
            return None;
        }
        let period = self.opts.period;
        let start = period.start(day);
        Some(self.by_start.entry(start).or_insert_with(|| ActivityGroup {
            label: period.label(start),
            start: start.iso(),
            tasks_done: Vec::new(),
            docs_changed: Vec::new(),
            decisions_changed: Vec::new(),
            commit_count: 0,
        }))
    }

    fn add_change(&mut self, snapshot: &Snapshot, entry: &Entry, item: &Item, day: &str) {
        let is_decision = item.item_type() == Some(ItemType::Decision);
        let Some(group) = self.at(day) else {
            return;
        };
        let list = if is_decision {
            &mut group.decisions_changed
        } else {
            &mut group.docs_changed
        };
        let id = snapshot.label(entry);
        let day = day.get(..10).unwrap_or(day);
        match list.iter_mut().find(|change| change.id == id) {
            Some(change) if change.day.as_str() < day => change.day = day.to_string(),
            Some(_) => {}
            None => list.push(ChangedItem {
                title: item.fields.title.clone().unwrap_or_else(|| id.clone()),
                id,
                day: day.to_string(),
            }),
        }
    }
}

/// The docs and decisions of the store, by file path.
fn readable(snapshot: &Snapshot) -> BTreeMap<&str, (&Entry, &Item)> {
    snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let item = entry.item.as_ref()?;
            matches!(item.item_type(), Some(ItemType::Doc | ItemType::Decision))
                .then_some((entry.path.as_str(), (entry, item)))
        })
        .collect()
}

/// Work done since `from`, grouped by period, newest first. A period with nothing in it is left out.
pub fn activity(repo: &Repo, opts: &ActivityOpts) -> Result<Vec<ActivityGroup>, EngineError> {
    let snapshot = repo.snapshot()?;
    let links = WorkLinks::load(repo, &snapshot)?;
    let from = opts
        .period
        .start(opts.from.unwrap_or_else(|| opts.period.default_from(Date::today())));
    let mut groups = Groups {
        opts: *opts,
        from,
        by_start: BTreeMap::new(),
    };

    for (entry, item) in tasks(&snapshot) {
        let Some(closed) = links.closed(entry, item) else {
            continue;
        };
        if let Some(group) = groups.at(&closed.day) {
            let id = snapshot.label(entry);
            group.tasks_done.push(DoneTask {
                title: item.fields.title.clone().unwrap_or_else(|| id.clone()),
                id,
                closed: closed.day,
                estimated: closed.estimated,
            });
        }
    }

    let items = readable(&snapshot);
    if let Some(index) = index_with_history(repo)? {
        let conn = index.conn();
        let mut stmt = conn
            .prepare("SELECT substr(date, 1, 10), count(*) FROM commits WHERE substr(date, 1, 10) >= ?1 GROUP BY 1")?;
        let days = stmt.query_map(params![from.iso()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?;
        for row in days {
            let (day, count) = row?;
            if let Some(group) = groups.at(&day) {
                group.commit_count += count;
            }
        }
        let mut stmt = conn.prepare(
            "SELECT DISTINCT substr(c.date, 1, 10), f.path FROM commit_files f \
             JOIN commits c ON c.repo = f.repo AND c.sha = f.sha WHERE substr(c.date, 1, 10) >= ?1",
        )?;
        let changes = stmt.query_map(params![from.iso()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in changes {
            let (day, path) = row?;
            if let Some((entry, item)) = items.get(path.as_str()) {
                groups.add_change(&snapshot, entry, item, &day);
            }
        }
    }
    for path in uncommitted(repo, items.keys().copied()) {
        if let (Some((entry, item)), Some(day)) = (items.get(path), links.updated(path)) {
            groups.add_change(&snapshot, entry, item, day);
        }
    }

    let mut out: Vec<ActivityGroup> = groups.by_start.into_values().rev().collect();
    for group in &mut out {
        group
            .tasks_done
            .sort_by(|a, b| b.closed.cmp(&a.closed).then_with(|| a.id.cmp(&b.id)));
        for list in [&mut group.docs_changed, &mut group.decisions_changed] {
            list.sort_by(|a, b| b.day.cmp(&a.day).then_with(|| a.title.cmp(&b.title)));
        }
    }
    Ok(out)
}
