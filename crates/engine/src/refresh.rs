//! Brings `.stone/index.db` in line with the item files. The index is derived,
//! so refresh never takes the store lock and never writes under `stone/`.
//!
//! A file whose mtime and size match its `files` row is skipped without being
//! read. The others are hashed; only a changed hash replaces that file's rows,
//! all in one `BEGIN IMMEDIATE` transaction per batch of files. The `files`
//! rows double as the resume cursor: an interrupted fill continues with the
//! files that do not match.

use std::collections::{HashMap, HashSet};

use carvestone_index::Index;
use carvestone_store::FileStat;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::links::{LinkContext, links_of};
use crate::repo::{EngineError, Repo};
use crate::units::units_of;

/// Files per write transaction, so a large first fill never holds the write
/// lock past the busy timeout.
pub(crate) const BATCH_FILES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefreshReport {
    pub scanned: usize,
    pub changed: usize,
    pub removed: usize,
    /// Code and project files whose graph rows were replaced or dropped.
    pub code_changed: usize,
    /// Commits added to the git cache.
    pub git_commits: usize,
    /// Repos whose history git could not read, one plain sentence each; the others are filled.
    pub git_broken: Vec<String>,
    /// The history cache holds every commit of every repo.
    pub git_complete: bool,
}

struct Known {
    mtime_ns: i64,
    size: i64,
}

pub fn refresh(repo: &Repo) -> Result<RefreshReport, EngineError> {
    refresh_batched(repo, BATCH_FILES, None)
}

/// `refresh` with its batch size, optionally stopping after `stop_after`
/// batches. Tests use it to interrupt a fill; the rows stay consistent either way.
#[doc(hidden)]
pub fn refresh_batched(
    repo: &Repo,
    batch_files: usize,
    stop_after: Option<usize>,
) -> Result<RefreshReport, EngineError> {
    refresh_in(repo, &repo.index()?, batch_files, stop_after)
}

/// A refresh that does not ask the repo's `Freshness` first; that gate calls this itself.
pub(crate) fn refresh_unguarded(repo: &Repo) -> Result<RefreshReport, EngineError> {
    refresh_now(repo, &repo.index()?, BATCH_FILES, None)
}

/// Refresh through an index the caller already opened, so a search opens it once. With a
/// `Freshness` on the repo this returns an empty report when the index is known to be current.
pub(crate) fn refresh_in(
    repo: &Repo,
    index: &Index,
    batch_files: usize,
    stop_after: Option<usize>,
) -> Result<RefreshReport, EngineError> {
    match repo.freshness() {
        Some(gate) => gate.through(repo, || refresh_now(repo, index, batch_files, stop_after)),
        None => refresh_now(repo, index, batch_files, stop_after),
    }
}

fn refresh_now(
    repo: &Repo,
    index: &Index,
    batch_files: usize,
    stop_after: Option<usize>,
) -> Result<RefreshReport, EngineError> {
    let conn = index.conn();
    // Doc-root files are sections like any other; they are only read, never written.
    let mut files = repo.store.stat_files()?;
    files.extend(repo.store.stat_docs()?);
    let known = known_files(conn)?;

    let on_disk: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut stale: Vec<FileStat> = files
        .iter()
        .filter(|f| {
            known
                .get(&f.path)
                .is_none_or(|k| k.mtime_ns != f.mtime_ns || k.size != f.size as i64)
        })
        .cloned()
        .collect();
    let mut gone: Vec<String> = known
        .keys()
        .filter(|p| !on_disk.contains(p.as_str()))
        .cloned()
        .collect();
    gone.sort();
    if let Some(batches) = stop_after {
        stale.truncate(batches * batch_files);
        gone.truncate(batches * batch_files);
    }

    let mut report = RefreshReport {
        scanned: files.len(),
        ..RefreshReport::default()
    };
    let links = LinkContext::new(&repo.store.config);
    in_batches(conn, stale, batch_files, |tx, batch| {
        for file in batch {
            if index_file(repo, &links, tx, file)? {
                report.changed += 1;
            }
        }
        Ok(())
    })?;
    in_batches(conn, gone, batch_files, |tx, batch| {
        for path in batch {
            if let Some(id) = file_id(tx, path)? {
                delete_file(tx, id)?;
                report.removed += 1;
            }
        }
        Ok(())
    })?;
    report.code_changed = crate::code_index::refresh_code(repo, conn, batch_files, stop_after)?;
    if report.code_changed > 0 {
        crate::code_native::restore(repo, conn)?;
    }
    crate::modules::recompute_if_stale(conn)?;
    // History trouble (no git, an old git) is for the git features to report.
    if let Ok(git) = crate::git_cache::refresh_git(repo, conn, crate::git_cache::BATCH_COMMITS, stop_after)? {
        report.git_commits = git.added;
        report.git_broken = git.broken;
        report.git_complete = git.complete;
    }
    Ok(report)
}

/// Runs `f` over `items` in chunks of `size`, each in its own immediate
/// transaction. Later fills (graph, git) use the same helper.
pub(crate) fn in_batches<T>(
    conn: &Connection,
    items: Vec<T>,
    size: usize,
    mut f: impl FnMut(&Transaction, &[T]) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    for chunk in items.chunks(size.max(1)) {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        f(&tx, chunk)?;
        tx.commit()?;
    }
    Ok(())
}

fn known_files(conn: &Connection) -> Result<HashMap<String, Known>, EngineError> {
    // Code and project files (lang set) belong to the code pass.
    let mut stmt = conn.prepare("SELECT path, mtime_ns, size FROM files WHERE lang IS NULL")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Known {
                mtime_ns: row.get(1)?,
                size: row.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn file_id(tx: &Transaction, path: &str) -> Result<Option<i64>, EngineError> {
    Ok(tx
        .query_row("SELECT id FROM files WHERE path = ?1", [path], |row| row.get(0))
        .optional()?)
}

/// Re-checks the file inside the transaction, so a concurrent refresh that
/// already did the work is not repeated. True when rows were replaced.
fn index_file(repo: &Repo, links: &LinkContext, tx: &Transaction, file: &FileStat) -> Result<bool, EngineError> {
    let row: Option<(i64, String, i64, i64)> = tx
        .query_row(
            "SELECT id, hash, mtime_ns, size FROM files WHERE path = ?1",
            [&file.path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((_, _, mtime_ns, size)) = &row
        && *mtime_ns == file.mtime_ns
        && *size == file.size as i64
    {
        return Ok(false);
    }

    let entry = repo.store.read_entry(file)?;
    let hash = blake3::hash(entry.text.as_bytes()).to_hex().to_string();
    if let Some((id, old_hash, _, _)) = row {
        if old_hash == hash {
            tx.execute(
                "UPDATE files SET mtime_ns = ?1, size = ?2 WHERE id = ?3",
                params![file.mtime_ns, file.size as i64, id],
            )?;
            return Ok(false);
        }
        delete_file(tx, id)?;
    }

    tx.execute(
        "INSERT INTO files(path, hash, mtime_ns, size, item_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![file.path, hash, file.mtime_ns, file.size as i64, entry.id.as_str()],
    )?;
    let file_id = tx.last_insert_rowid();
    let Some(item) = &entry.item else {
        return Ok(true);
    };

    let kind = if file.plain {
        "doc".to_string()
    } else {
        item.type_name.clone().unwrap_or_default()
    };
    let title = item.fields.title.clone().unwrap_or_default();
    let mut section = tx.prepare_cached(
        "INSERT INTO sections(file_id, item_id, anchor, title, level, line_start, line_end, kind, status, milestone, text) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    let mut fts = tx.prepare_cached("INSERT INTO fts(rowid, title, heading, body) VALUES (?1, ?2, ?3, ?4)")?;
    let units = units_of(item);
    for unit in &units {
        section.execute(params![
            file_id,
            entry.id.as_str(),
            unit.anchor,
            unit.title,
            unit.level,
            unit.line_start,
            unit.line_end,
            kind,
            item.fields.status,
            item.fields.milestone,
            unit.text,
        ])?;
        let heading = if unit.anchor.is_empty() {
            ""
        } else {
            unit.title.as_str()
        };
        fts.execute(params![tx.last_insert_rowid(), title, heading, unit.text])?;
    }

    let mut link = tx.prepare_cached(
        "INSERT INTO links(src_item, src_anchor, dst_item, dst_anchor, kind, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for row in links_of(links, &entry, &units) {
        link.execute(params![
            entry.id.as_str(),
            row.src_anchor,
            row.dst_item,
            row.dst_anchor,
            row.kind,
            row.line
        ])?;
    }
    Ok(true)
}

/// Drops every row that belongs to the file.
fn delete_file(tx: &Transaction, id: i64) -> Result<(), EngineError> {
    tx.execute(
        "DELETE FROM fts WHERE rowid IN (SELECT id FROM sections WHERE file_id = ?1)",
        [id],
    )?;
    tx.execute(
        "DELETE FROM links WHERE src_item = (SELECT item_id FROM files WHERE id = ?1)",
        [id],
    )?;
    tx.execute("DELETE FROM sections WHERE file_id = ?1", [id])?;
    tx.execute("DELETE FROM files WHERE id = ?1", [id])?;
    Ok(())
}
