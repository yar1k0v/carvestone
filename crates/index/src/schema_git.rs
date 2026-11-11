//! The git cache tables (`commits`, `commit_files`, `commit_refs`, `blame`).
//!
//! Kept apart from the main schema so the history cache is one self-contained
//! step; `schema::create` runs it right after the main tables.

use rusqlite::Connection;

use crate::IndexError;

pub(crate) fn create(conn: &Connection) -> Result<(), IndexError> {
    conn.execute_batch(SQL)?;
    Ok(())
}

// The cache is keyed by (repo, commit hash): `repo` is the git repo's folder relative
// to the product root, `''` for the root itself, so one index holds the history of
// every repo. `ts` is the author date in Unix seconds, `parents` a space-separated
// list. Rows are filled by the engine from the git CLI and never trusted over it.
pub(crate) const SQL: &str = r#"
CREATE TABLE commits (
    repo    TEXT NOT NULL DEFAULT '',
    sha     TEXT NOT NULL,
    date    TEXT NOT NULL,
    ts      INTEGER NOT NULL,
    author  TEXT NOT NULL,
    subject TEXT NOT NULL,
    parents TEXT NOT NULL,
    PRIMARY KEY (repo, sha)
) WITHOUT ROWID;
CREATE INDEX commits_ts ON commits(ts);

-- `status` is one letter (A, M, D, R, C, T); `old_path` is set for R and C. Both
-- paths are relative to the product root, not to the repo.
CREATE TABLE commit_files (
    repo     TEXT NOT NULL DEFAULT '',
    sha      TEXT NOT NULL,
    path     TEXT NOT NULL,
    status   TEXT NOT NULL,
    old_path TEXT,
    FOREIGN KEY (repo, sha) REFERENCES commits(repo, sha) ON DELETE CASCADE
);
CREATE INDEX commit_files_sha ON commit_files(repo, sha);
CREATE INDEX commit_files_path ON commit_files(path);

-- `kind` is task, plan or decision. `raw` is the trailer value as written;
-- `ref_id` is the item id it resolves to, or `raw` when nothing resolves.
CREATE TABLE commit_refs (
    repo   TEXT NOT NULL DEFAULT '',
    sha    TEXT NOT NULL,
    kind   TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    raw    TEXT NOT NULL,
    FOREIGN KEY (repo, sha) REFERENCES commits(repo, sha) ON DELETE CASCADE
);
CREATE INDEX commit_refs_sha ON commit_refs(repo, sha);
CREATE INDEX commit_refs_ref ON commit_refs(ref_id);

-- Blame of a file that equals HEAD, one row per run of lines from the same
-- commit, keyed by the file's blob hash. `path` is relative to the product root;
-- `sha` is a commit of `repo`. `orig_start` and `orig_path` locate the first line
-- in the commit that introduced it, `orig_path` relative to the repo as git says.
CREATE TABLE blame (
    repo         TEXT NOT NULL DEFAULT '',
    path         TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    line_start   INTEGER NOT NULL,
    line_end     INTEGER NOT NULL,
    sha          TEXT NOT NULL,
    orig_start   INTEGER NOT NULL,
    orig_path    TEXT NOT NULL,
    PRIMARY KEY (path, content_hash, line_start)
) WITHOUT ROWID;
"#;
