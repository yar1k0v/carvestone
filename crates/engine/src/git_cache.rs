//! The git cache inside refresh: `commits`, `commit_files`, `commit_refs`
//! and `blame`, derived from the `git` CLI and never trusted over it.
//!
//! A first fill (or a refill after rewritten history) takes every commit
//! reachable from HEAD, oldest first, in batches of 500 with one transaction
//! each. `meta.git_target`, `meta.git_base` and `meta.git_filled` record the
//! fill in progress, so a fill killed after any batch resumes where it was;
//! `meta.git_head` is set only when the fill is complete. An old head that is
//! an ancestor of HEAD appends `old..HEAD` the same way; one that is not
//! (rebase, amend, another branch) clears the commit tables and refills. So
//! does a change of the clone's cut (`meta.git_shallow`, the sorted
//! `.git/shallow` shas) after `fetch --deepen` or `--unshallow`.
//!
//! Every decision is made under the write lock and recorded in the same
//! transaction; each batch checks the marks again under its own lock, so two
//! refreshes (the watcher and a request) never write into each other's fill.
//!
//! Each git repo of the product (`Repo::repos()`) is filled on its own: rows carry the
//! repo's folder, and the marks above are suffixed with it (`git_head:api`; the root repo
//! keeps the bare names). A repo whose HEAD did not move costs one `rev-parse`. File
//! paths are stored relative to the product root. Rows of a repo that is no longer found
//! are dropped.
//!
//! Outside git the tables stay empty. A git problem never fails `refresh`;
//! the features that need history call `ensure_filled` and report it.

use std::collections::{HashMap, HashSet};

use carvestone_core::{IdScheme, ItemId, NumberedKind};
use carvestone_git::{BlameLine, Commit, Git, GitError};
use carvestone_index::Index;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::repo::{EngineError, Repo};
use crate::repos::{RepoRef, Repos};

/// Commits per write transaction.
pub(crate) const BATCH_COMMITS: usize = 500;

const KEYS: [&str; 5] = ["git_head", "git_target", "git_base", "git_filled", "git_shallow"];

/// The meta key of a mark for the repo in `dir`: bare for the root repo, else `name:dir`.
fn mark(name: &str, dir: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{name}:{dir}")
    }
}

/// Plans that restart because another refresh moved the fill under us, before the fill reports
/// itself incomplete.
const MAX_REPLANS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitReport {
    /// Commits written by this call.
    pub added: usize,
    /// The commit tables were emptied because history was rewritten or git went away.
    pub cleared: bool,
    /// The cache holds every commit reachable from HEAD in each repo git could read.
    pub complete: bool,
    /// One plain sentence per repo git could not read; the others are filled regardless.
    pub broken: Vec<String>,
}

enum Fail {
    Git(GitError),
    Engine(EngineError),
}

impl From<GitError> for Fail {
    fn from(err: GitError) -> Self {
        Fail::Git(err)
    }
}

impl From<rusqlite::Error> for Fail {
    fn from(err: rusqlite::Error) -> Self {
        Fail::Engine(err.into())
    }
}

impl From<EngineError> for Fail {
    fn from(err: EngineError) -> Self {
        Fail::Engine(err)
    }
}

/// Brings the commit tables in line with HEAD, stopping after `stop_after`
/// batches when given. The outer error is the engine's (SQLite, I/O); the
/// inner one is git's, for callers that must say why history is missing.
pub(crate) fn refresh_git(
    repo: &Repo,
    conn: &Connection,
    batch: usize,
    stop_after: Option<usize>,
) -> Result<Result<GitReport, GitError>, EngineError> {
    let repos = repo.repos();
    let resolver = Resolver::load(conn, repo.store.config.scheme())?;
    let mut total = GitReport {
        cleared: forget_gone(conn, &repos)?,
        complete: true,
        ..GitReport::default()
    };
    // No repo anywhere: the tables are empty and the caller hears why.
    if repos.all().is_empty() {
        return Ok(Err(GitError::Unavailable));
    }
    let mut failed = None;
    let mut batches = 0;
    for one in repos.all() {
        match sync(&repos, one, conn, &resolver, batch.max(1), &mut batches, stop_after) {
            Ok(report) => {
                total.added += report.added;
                total.cleared |= report.cleared;
                total.complete &= report.complete;
            }
            // One broken repo does not keep the others from filling or from being answered.
            Err(Fail::Git(err)) => {
                let name = if one.dir().is_empty() {
                    "the product root"
                } else {
                    one.dir()
                };
                total.broken.push(format!(
                    "history of {name} is missing: {err}; fix git in that folder and the next refresh picks it up"
                ));
                failed.get_or_insert(err);
            }
            Err(Fail::Engine(err)) => return Err(err),
        }
    }
    // Only when no repo could be read is git itself the problem.
    if total.broken.len() == repos.all().len()
        && let Some(err) = failed
    {
        return Ok(Err(err));
    }
    recanonicalise(conn, &resolver)?;
    Ok(Ok(total))
}

/// Drops the rows and marks of repos that are no longer found, so a repo removed from the
/// config leaves no history behind. True when anything was dropped.
fn forget_gone(conn: &Connection, repos: &Repos) -> Result<bool, EngineError> {
    let mut known: HashSet<String> = HashSet::new();
    for sql in ["SELECT DISTINCT repo FROM commits", "SELECT DISTINCT repo FROM blame"] {
        let rows = conn
            .prepare(sql)?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        known.extend(rows);
    }
    let keys = conn
        .prepare("SELECT key FROM meta WHERE key LIKE 'git\\_%:%' ESCAPE '\\'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    known.extend(
        keys.iter()
            .filter_map(|k| k.split_once(':').map(|(_, dir)| dir.to_string())),
    );
    let mut gone: Vec<String> = known
        .into_iter()
        .filter(|dir| !repos.all().iter().any(|r| r.dir() == dir))
        .collect();
    gone.sort();
    let mut dropped = false;
    for dir in gone {
        dropped |= locked(conn, |tx| Ok(clear(tx, &dir)?)).map_err(|fail| match fail {
            Fail::Engine(err) => err,
            Fail::Git(err) => err.into(),
        })?;
    }
    Ok(dropped)
}

/// Completes the cache fill so history answers are whole; git trouble is an error here.
pub fn ensure_filled(repo: &Repo) -> Result<GitReport, EngineError> {
    let index = repo.index()?;
    Ok(refresh_git(repo, index.conn(), BATCH_COMMITS, None)??)
}

/// Completes the fill and says whether the cache now holds the whole history. Outside git, or on
/// a git problem, it is `false` and the caller skips its history rule without a word.
pub(crate) fn history_ready(repo: &Repo, conn: &Connection) -> Result<bool, EngineError> {
    if let Some(ready) = repo.freshness().and_then(|gate| gate.history_complete()) {
        return Ok(ready);
    }
    Ok(matches!(
        refresh_git(repo, conn, BATCH_COMMITS, None)?,
        Ok(report) if report.complete
    ))
}

/// The index after a refresh, with the history cache whole; `None` when there is no history to
/// read (no git) or no index can be written in this checkout.
pub(crate) fn index_with_history(repo: &Repo) -> Result<Option<Index>, EngineError> {
    match crate::refresh::refresh(repo) {
        Ok(_) => {}
        Err(EngineError::Invalid(msg)) if msg.starts_with("cannot write .stone/") => return Ok(None),
        Err(err) => return Err(err),
    }
    let index = repo.index()?;
    Ok(history_ready(repo, index.conn())?.then_some(index))
}

/// Fills the cache for one repo. `batches` counts the batches written over all repos of this
/// refresh, so `stop_after` bounds the whole refresh.
fn sync(
    repos: &Repos,
    one: &RepoRef,
    conn: &Connection,
    resolver: &Resolver,
    batch: usize,
    batches: &mut usize,
    stop_after: Option<usize>,
) -> Result<GitReport, Fail> {
    let git = one.git();
    let dir = one.dir();
    let mut report = GitReport::default();
    'replan: for _ in 0..MAX_REPLANS {
        if is_current(git, conn, dir) {
            report.complete = true;
            return Ok(report);
        }
        // The decision and the progress marks are written under one write lock, so a
        // second refresh sees either the old state or the whole new plan.
        let fill = match locked(conn, |tx| plan(git, tx, dir))? {
            Plan::Done { cleared } => {
                report.cleared |= cleared;
                report.complete = true;
                return Ok(report);
            }
            Plan::NoGit => return Err(GitError::Unavailable.into()),
            Plan::Fill(fill) => fill,
        };
        report.cleared |= fill.cleared;

        let mut filled = fill.filled;
        while filled < fill.shas.len() {
            if stop_after.is_some_and(|n| *batches >= n) {
                return Ok(report);
            }
            let end = (filled + batch).min(fill.shas.len());
            // Git runs before the write lock is taken, so the lock is held only for rows.
            let commits = git.commits(&fill.shas[filled..end])?;
            let added = locked(conn, |tx| {
                // Another refresh cleared, advanced or finished this fill: plan again.
                let ours = meta_get(tx, &mark("git_target", dir))?.as_deref() == Some(fill.head.as_str())
                    && meta_get(tx, &mark("git_filled", dir))?.and_then(|n| n.parse::<usize>().ok()) == Some(filled);
                if !ours {
                    return Ok(None);
                }
                let added = insert_commits(tx, repos, one, &commits, resolver)?;
                meta_set(tx, &mark("git_filled", dir), &end.to_string())?;
                if end == fill.shas.len() {
                    finish(tx, dir, &fill.head)?;
                }
                Ok(Some(added))
            })?;
            let Some(added) = added else {
                continue 'replan;
            };
            report.added += added;
            filled = end;
            *batches += 1;
        }
        report.complete = true;
        return Ok(report);
    }
    // Other refreshes kept moving the fill; the next call picks it up.
    Ok(report)
}

/// What the write-locked look at git and the cache decided.
enum Plan {
    /// The cache already matches (or was emptied for) HEAD.
    Done {
        cleared: bool,
    },
    /// Git cannot work here; the cache was emptied.
    NoGit,
    Fill(Fill),
}

struct Fill {
    head: String,
    /// Commits still to read in full, oldest first; `filled` of them are done.
    shas: Vec<String>,
    filled: usize,
    cleared: bool,
}

/// Whether the cache is complete for the current HEAD and clone depth. Reads only; a
/// stale answer is no worse than the state a moment ago.
fn is_current(git: &Git, conn: &Connection, dir: &str) -> bool {
    let (Ok(Some(head)), Ok(shallow)) = (git.head(), git.shallow_roots()) else {
        return false;
    };
    let Ok(tx) = Transaction::new_unchecked(conn, TransactionBehavior::Deferred) else {
        return false;
    };
    let state = (|| {
        Ok::<_, rusqlite::Error>((
            meta_get(&tx, &mark("git_target", dir))?,
            meta_get(&tx, &mark("git_head", dir))?,
            meta_get(&tx, &mark("git_shallow", dir))?.unwrap_or_default(),
        ))
    })();
    matches!(state, Ok((None, Some(done), seen)) if done == head && seen == shallow.join(" "))
}

/// Decides what a refresh does, with the write lock held: reads the cache's marks, clears
/// what no longer fits HEAD and records the target of the fill.
fn plan(git: &Git, tx: &Connection, dir: &str) -> Result<Plan, Fail> {
    let head = match git.head() {
        Ok(Some(head)) => head,
        // A branch without commits: the tables stay empty.
        Ok(None) => {
            let cleared = clear(tx, dir)?;
            return Ok(Plan::Done { cleared });
        }
        // No git: empty too, and the caller hears why.
        Err(GitError::Unavailable) => {
            clear(tx, dir)?;
            return Ok(Plan::NoGit);
        }
        Err(err) => return Err(err.into()),
    };
    let shallow = git.shallow_roots()?.join(" ");

    let mut cleared = false;
    let target = meta_get(tx, &mark("git_target", dir))?;
    let done = meta_get(tx, &mark("git_head", dir))?;
    let (base, start) = if meta_get(tx, &mark("git_shallow", dir))?.unwrap_or_default() != shallow {
        // A fetch moved the clone's cut: boundary commits list other files and blame
        // runs end elsewhere, so nothing cached can be trusted.
        cleared = clear(tx, dir)?;
        (None, 0)
    } else {
        match (target.as_deref(), done) {
            (Some(target), _) if target == head => (
                meta_get(tx, &mark("git_base", dir))?.filter(|b| !b.is_empty()),
                meta_get(tx, &mark("git_filled", dir))?
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0),
            ),
            (Some(_), _) => {
                cleared = clear(tx, dir)?;
                (None, 0)
            }
            (None, Some(old)) if old == head => return Ok(Plan::Done { cleared: false }),
            (None, Some(old)) => {
                // A sha git no longer knows is as good as a rewritten history.
                if git.is_ancestor(&old, &head).unwrap_or(false) {
                    (Some(old), 0)
                } else {
                    cleared = clear(tx, dir)?;
                    (None, 0)
                }
            }
            (None, None) => {
                cleared = clear(tx, dir)?;
                (None, 0)
            }
        }
    };

    let range = match &base {
        Some(base) => format!("{base}..{head}"),
        None => head.clone(),
    };
    let shas = git.rev_list_reverse(&range)?;
    if meta_get(tx, &mark("git_target", dir))?.is_none() {
        meta_set(tx, &mark("git_target", dir), &head)?;
        meta_set(tx, &mark("git_base", dir), base.as_deref().unwrap_or(""))?;
        meta_set(tx, &mark("git_filled", dir), "0")?;
        if shallow.is_empty() {
            meta_del(tx, &mark("git_shallow", dir))?;
        } else {
            meta_set(tx, &mark("git_shallow", dir), &shallow)?;
        }
    }
    let filled = start.min(shas.len());
    if filled == shas.len() {
        finish(tx, dir, &head)?;
        return Ok(Plan::Done { cleared });
    }
    Ok(Plan::Fill(Fill {
        head,
        shas,
        filled,
        cleared,
    }))
}

/// Runs `f` in an immediate transaction: the write lock is taken first, so what `f` reads
/// is still true when it writes. A failure rolls everything back.
fn locked<T>(conn: &Connection, f: impl FnOnce(&Transaction) -> Result<T, Fail>) -> Result<T, Fail> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let value = f(&tx)?;
    tx.commit()?;
    Ok(value)
}

/// The fill is whole: record HEAD and drop the progress marks.
fn finish(conn: &Connection, dir: &str, head: &str) -> Result<(), rusqlite::Error> {
    meta_set(conn, &mark("git_head", dir), head)?;
    meta_del(conn, &mark("git_target", dir))?;
    meta_del(conn, &mark("git_base", dir))?;
    let total: i64 = conn.query_row("SELECT count(*) FROM commits WHERE repo = ?1", [dir], |row| row.get(0))?;
    meta_set(conn, &mark("git_filled", dir), &total.to_string())
}

/// Empties the git tables and marks of the repo in `dir`. True when there was anything to
/// drop. The caller holds the write lock.
fn clear(conn: &Connection, dir: &str) -> Result<bool, rusqlite::Error> {
    let keys: Vec<String> = KEYS.iter().map(|k| mark(k, dir)).collect();
    let has_rows: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM commits WHERE repo = ?1) OR EXISTS(SELECT 1 FROM blame WHERE repo = ?1) \
         OR EXISTS(SELECT 1 FROM meta WHERE key IN (?2, ?3, ?4, ?5, ?6))",
        params![dir, keys[0], keys[1], keys[2], keys[3], keys[4]],
        |row| row.get(0),
    )?;
    if !has_rows {
        return Ok(false);
    }
    conn.execute("DELETE FROM commits WHERE repo = ?1", [dir])?;
    conn.execute("DELETE FROM blame WHERE repo = ?1", [dir])?;
    for key in &keys {
        meta_del(conn, key)?;
    }
    Ok(true)
}

fn insert_commits(
    tx: &Transaction,
    repos: &Repos,
    one: &RepoRef,
    commits: &[Commit],
    resolver: &Resolver,
) -> Result<usize, rusqlite::Error> {
    let dir = one.dir();
    let mut commit = tx.prepare_cached(
        "INSERT OR IGNORE INTO commits(repo, sha, date, ts, author, subject, parents) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut file =
        tx.prepare_cached("INSERT INTO commit_files(repo, sha, path, status, old_path) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    let mut reference =
        tx.prepare_cached("INSERT INTO commit_refs(repo, sha, kind, ref_id, raw) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    // A commit that changes a file changes its blame: drop what was cached for it.
    let mut stale_blame = tx.prepare_cached("DELETE FROM blame WHERE path = ?1")?;

    let mut added = 0;
    for c in commits {
        let parents = c.parents.join(" ");
        if commit.execute(params![dir, c.sha, c.date, c.timestamp, c.author, c.subject, parents])? == 0 {
            continue;
        }
        added += 1;
        for f in &c.files {
            let path = repos.to_root(one, &f.path);
            let old_path = f.old_path.as_deref().map(|old| repos.to_root(one, old));
            file.execute(params![dir, c.sha, path, f.status.to_string(), old_path])?;
            stale_blame.execute([&path])?;
            if let Some(old) = &old_path {
                stale_blame.execute([old])?;
            }
        }
        let mut seen = HashSet::new();
        for (key, value) in &c.trailers {
            let Some(kind) = trailer_kind(key) else {
                continue;
            };
            if seen.insert((kind, value.clone())) {
                reference.execute(params![dir, c.sha, kind, resolver.canon(value), value])?;
            }
        }
    }
    Ok(added)
}

fn trailer_kind(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "refs" => Some("task"),
        "plan" => Some("plan"),
        "decision" => Some("decision"),
        _ => None,
    }
}

/// Items can appear or vanish after a commit was cached, so `ref_id` is
/// re-resolved on every refresh; only rows whose answer changed are written.
fn recanonicalise(conn: &Connection, resolver: &Resolver) -> Result<(), rusqlite::Error> {
    let pairs: Vec<(String, String)> = conn
        .prepare("SELECT DISTINCT raw, ref_id FROM commit_refs")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let changed: Vec<(String, String)> = pairs
        .into_iter()
        .filter_map(|(raw, id)| {
            let now = resolver.canon(&raw);
            (now != id).then_some((raw, now))
        })
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    for (raw, now) in &changed {
        tx.execute(
            "UPDATE commit_refs SET ref_id = ?2 WHERE raw = ?1 AND ref_id <> ?2",
            params![raw, now],
        )?;
    }
    tx.commit()
}

// ------------------------------------------------------------- resolver

/// Trailer values to item ids, from the ids the index already holds.
struct Resolver {
    scheme: IdScheme,
    ids: HashSet<String>,
    /// Numbered items by kind and number, for a zero-padded file name.
    numbered: HashMap<(NumberedKind, u32), String>,
}

impl Resolver {
    fn load(conn: &Connection, scheme: IdScheme) -> Result<Resolver, rusqlite::Error> {
        let ids: Vec<String> = conn
            .prepare("SELECT DISTINCT item_id FROM files WHERE lang IS NULL ORDER BY item_id")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let mut numbered = HashMap::new();
        for id in &ids {
            if let Some(key) = scheme.number(&ItemId::from_path(id)) {
                numbered.entry(key).or_insert_with(|| id.clone());
            }
        }
        Ok(Resolver {
            scheme,
            ids: ids.into_iter().collect(),
            numbered,
        })
    }

    /// The item id `raw` names, or `raw` as written when nothing matches.
    fn canon(&self, raw: &str) -> String {
        let Ok(parsed) = self.scheme.parse(raw) else {
            return raw.to_string();
        };
        if self.ids.contains(parsed.id.as_str()) {
            return parsed.id.as_str().to_string();
        }
        match self.scheme.number(&parsed.id).and_then(|key| self.numbered.get(&key)) {
            Some(id) => id.clone(),
            None => raw.to_string(),
        }
    }
}

// ---------------------------------------------------------------- blame

/// Blame of `lines` (1-based, inclusive) in `path`, relative to the product root, from the repo
/// that holds it (the lines' `path` is relative to that repo, as git says it). A file equal to
/// HEAD is cached by its blob hash; a dirty file is blamed live and never stored. A path in no
/// repo is `GitError::Unavailable`.
pub fn cached_blame(repo: &Repo, path: &str, lines: (u32, u32)) -> Result<Vec<BlameLine>, EngineError> {
    let repos = repo.repos();
    let Some((one, inner)) = repos.repo_of(path) else {
        return Err(GitError::Unavailable.into());
    };
    let index = repo.index()?;
    let conn = index.conn();
    // New commits drop the blame of the files they touch, so catch up first.
    refresh_git(repo, conn, BATCH_COMMITS, None)??;

    let git = one.git();
    let blob = match git.head_blob(&inner)? {
        Some(blob) if !git.differs_from_head(&inner)? => blob,
        _ => return Ok(git.blame(&inner, Some(lines))?),
    };

    let path = repos.to_root(one, &inner);
    let cached = cached_runs(conn, &path, &blob, lines)?;
    if covers(&cached, lines) {
        return Ok(expand(&cached, lines));
    }
    let live = git.blame(&inner, Some(lines))?;
    store_runs(conn, one.dir(), &path, &blob, lines, &runs_of(&live))?;
    Ok(live)
}

/// Consecutive lines from one commit, one file and consecutive original lines.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    start: u32,
    end: u32,
    sha: String,
    orig_start: u32,
    orig_path: String,
}

fn runs_of(lines: &[BlameLine]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for line in lines {
        if let Some(last) = runs.last_mut()
            && last.sha == line.sha
            && last.orig_path == line.path
            && last.end + 1 == line.final_line
            && last.orig_start + (last.end - last.start + 1) == line.orig_line
        {
            last.end = line.final_line;
            continue;
        }
        runs.push(Run {
            start: line.final_line,
            end: line.final_line,
            sha: line.sha.clone(),
            orig_start: line.orig_line,
            orig_path: line.path.clone(),
        });
    }
    runs
}

fn cached_runs(conn: &Connection, path: &str, blob: &str, (from, to): (u32, u32)) -> Result<Vec<Run>, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT line_start, line_end, sha, orig_start, orig_path FROM blame \
         WHERE path = ?1 AND content_hash = ?2 AND line_end >= ?3 AND line_start <= ?4 ORDER BY line_start",
    )?;
    let rows = stmt.query_map(params![path, blob, from, to], |row| {
        Ok(Run {
            start: row.get(0)?,
            end: row.get(1)?,
            sha: row.get(2)?,
            orig_start: row.get(3)?,
            orig_path: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether `runs` leave no line of `lines` uncovered.
fn covers(runs: &[Run], (from, to): (u32, u32)) -> bool {
    let mut next = from;
    for run in runs {
        if run.start > next {
            return false;
        }
        next = next.max(run.end + 1);
    }
    next > to
}

fn expand(runs: &[Run], (from, to): (u32, u32)) -> Vec<BlameLine> {
    let mut out = Vec::new();
    for run in runs {
        for line in run.start.max(from)..=run.end.min(to) {
            out.push(BlameLine {
                sha: run.sha.clone(),
                orig_line: run.orig_start + (line - run.start),
                final_line: line,
                path: run.orig_path.clone(),
            });
        }
    }
    out
}

fn store_runs(
    conn: &Connection,
    repo: &str,
    path: &str,
    blob: &str,
    (from, to): (u32, u32),
    runs: &[Run],
) -> Result<(), EngineError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    // One blob per file is enough: an older one can no longer be asked for.
    tx.execute(
        "DELETE FROM blame WHERE path = ?1 AND content_hash <> ?2",
        params![path, blob],
    )?;
    tx.execute(
        "DELETE FROM blame WHERE path = ?1 AND content_hash = ?2 AND line_end >= ?3 AND line_start <= ?4",
        params![path, blob, from, to],
    )?;
    for run in runs {
        tx.execute(
            "INSERT INTO blame(path, content_hash, line_start, line_end, sha, orig_start, orig_path, repo) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                path,
                blob,
                run.start,
                run.end,
                run.sha,
                run.orig_start,
                run.orig_path,
                repo
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ----------------------------------------------------------------- meta

fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| row.get(0))
        .optional()
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn meta_del(conn: &Connection, key: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM meta WHERE key = ?1", [key])?;
    Ok(())
}
