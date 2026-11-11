//! Schema: one row per file and section, FTS5 over sections and symbols,
//! links, and the code tables (`symbols`, `refs`, `imports`, `edges`,
//! `projects`, `modules`).
//!
//! Everything here is derived; a version change rebuilds the whole file.

use rusqlite::Connection;

use crate::IndexError;

pub(crate) fn create(conn: &Connection, version: i64) -> Result<(), IndexError> {
    conn.execute_batch(SQL)?;
    crate::schema_git::create(conn)?;
    conn.execute("INSERT INTO meta(key, value) VALUES ('schema_version', ?1)", [version])?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES ('schema_hash', ?1)",
        [shape_hash()],
    )?;
    conn.execute("INSERT INTO meta(key, value) VALUES ('fingerprint', '')", [])?;
    // Code-graph counters. A rebuild starts with no code rows, so both
    // versions start at zero; the engine bumps them on every graph write.
    conn.execute_batch(
        "INSERT INTO meta(key, value) VALUES ('graph_version', '0');
         INSERT INTO meta(key, value) VALUES ('modules_version', '0');
         INSERT INTO meta(key, value) VALUES ('code_building', '0');",
    )?;
    Ok(())
}

pub(crate) fn version(conn: &Connection) -> Option<i64> {
    let text: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| {
            row.get(0)
        })
        .ok()?;
    text.parse().ok()
}

/// True when the file was written at `version` by a build with the same tables.
/// The number alone is not enough: two builds can both write version N with
/// different tables, and the file then fails on the first missing one.
pub(crate) fn matches(conn: &Connection, version: i64) -> bool {
    if self::version(conn) != Some(version) {
        return false;
    }
    let stored: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_hash'", [], |row| row.get(0))
        .ok();
    stored.as_deref() == Some(shape_hash().as_str())
}

/// FNV-1a over the table definitions, so any change to them invalidates every
/// file written by another build, with or without a version bump.
fn shape_hash() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in SQL.bytes().chain(crate::schema_git::SQL.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The tree-sitter edge `e` (a row of `edges`) that a native edge `n` replaces.
/// A site edge is replaced when a native edge of the same kind starts at the
/// same symbol on the same line and names the same target: by its target
/// symbol, or by the identifier the tree-sitter edge was written with (the
/// last segment of its `dst_name`, which is the written name for an unresolved
/// edge, `name:<scope>:<name>` for a bucket and a qualified name otherwise).
/// A dispatch edge has no site: it is replaced by the native pair it names.
/// A native edge with no target (`dst_id` null) means the indexer resolved the
/// site outside the repo: it hides the guesses at the site, never an exact edge.
/// `$extra` narrows which native edges count (`1` for all).
macro_rules! native_mask {
    ($extra:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM native_edges n WHERE n.src_id = e.src_id AND n.kind = e.kind AND ",
            $extra,
            " AND (n.dst_id IS NOT NULL OR e.exact = 0) AND (\
            (e.kind = 'dispatch' AND n.dst_id = e.dst_id) \
            OR (e.kind <> 'dispatch' AND n.file_id = e.file_id AND n.line = e.line AND (\
                n.dst_id = e.dst_id OR e.dst_name = n.name OR (\
                    length(e.dst_name) > length(n.name) AND substr(e.dst_name, -length(n.name)) = n.name \
                    AND substr(e.dst_name, -length(n.name) - 1, 1) IN ('.', ':', '/', '#')))))) "
        )
    };
}

/// Edge kinds that no native indexer writes and no guess can be left out for: a dispatch guess
/// (not every indexer writes implementations).
macro_rules! kept_guess {
    () => {
        "e.kind = 'dispatch'"
    };
}

/// True when a native run had the file `$file` (an expression for its id).
macro_rules! covered {
    ($file:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM native_files nf WHERE nf.file_id = ",
            $file,
            " AND nf.covered = 1)"
        )
    };
}

/// The FTS table is written and cleared by refresh next to the `sections`
/// rows, not by triggers: a reindexed file replaces both in one transaction.
const SQL: &str = concat!(
    r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE files (
    id       INTEGER PRIMARY KEY,
    path     TEXT NOT NULL UNIQUE,
    hash     TEXT NOT NULL,
    mtime_ns INTEGER NOT NULL,
    size     INTEGER NOT NULL,
    item_id  TEXT NOT NULL,
    lang     TEXT
);

CREATE TABLE sections (
    id         INTEGER PRIMARY KEY,
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    item_id    TEXT NOT NULL,
    anchor     TEXT NOT NULL,
    title      TEXT NOT NULL,
    level      INTEGER NOT NULL,
    line_start INTEGER NOT NULL,
    line_end   INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    status     TEXT,
    milestone  TEXT,
    text       TEXT NOT NULL
);
CREATE INDEX sections_item ON sections(item_id);
CREATE INDEX sections_file ON sections(file_id);

CREATE VIRTUAL TABLE fts USING fts5(
    title,
    heading,
    body,
    tokenize = "unicode61 tokenchars '_-'"
);

CREATE TABLE links (
    src_item   TEXT NOT NULL,
    src_anchor TEXT NOT NULL,
    dst_item   TEXT NOT NULL,
    dst_anchor TEXT,
    kind       TEXT NOT NULL,
    line       INTEGER NOT NULL
);
CREATE INDEX links_dst ON links(dst_item);
CREATE INDEX links_src ON links(src_item);

CREATE TABLE symbols (
    id         INTEGER PRIMARY KEY,
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    qname      TEXT NOT NULL,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    parent_id  INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    signature  TEXT NOT NULL,
    doc        TEXT,
    line_start INTEGER NOT NULL,
    line_end   INTEGER NOT NULL,
    lang       TEXT NOT NULL,
    project    TEXT
);
CREATE INDEX symbols_file ON symbols(file_id);
CREATE INDEX symbols_qname ON symbols(qname);
CREATE INDEX symbols_name ON symbols(name);
CREATE INDEX symbols_parent ON symbols(parent_id);

-- One row per reference site. `src_symbol_id` is the declaration the site
-- sits in, null when the site sits directly in the file.
CREATE TABLE refs (
    id            INTEGER PRIMARY KEY,
    file_id       INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    src_symbol_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    qualifier     TEXT,
    kind          TEXT NOT NULL,
    line          INTEGER NOT NULL
);
CREATE INDEX refs_file ON refs(file_id);
CREATE INDEX refs_name ON refs(name);
CREATE INDEX refs_symbol ON refs(src_symbol_id);

-- A name bucket: the candidates an untyped site with several of them points
-- at. `scope` is the visibility the fallback was computed in (a crate or
-- project path, `*` for the whole workspace), so a name shared by a thousand
-- declarations costs one bucket instead of a thousand edges per call site.
CREATE TABLE buckets (
    id    INTEGER PRIMARY KEY,
    lang  TEXT NOT NULL,
    scope TEXT NOT NULL,
    name  TEXT NOT NULL,
    UNIQUE(lang, scope, name)
);

-- Which declarations a bucket holds. Rewritten as a whole when the set
-- changes, never touching the files whose sites point at the bucket.
CREATE TABLE bucket_members (
    bucket_id INTEGER NOT NULL REFERENCES buckets(id) ON DELETE CASCADE,
    symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    PRIMARY KEY (bucket_id, symbol_id)
) WITHOUT ROWID;
CREATE INDEX bucket_members_symbol ON bucket_members(symbol_id);

-- `dst_id` is null for a reference the resolver could not pin to one
-- definition (an `inferred` edge). Edges have no id: a re-resolved file
-- deletes and rewrites all of its rows. An edge into a name bucket has
-- `dst_bucket` set and `dst_id` null: its real targets are the bucket's
-- members, so a shared name costs one row per site and one per declaration.
CREATE TABLE edges (
    src_id     INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    dst_id     INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    dst_bucket INTEGER REFERENCES buckets(id) ON DELETE CASCADE,
    dst_name   TEXT NOT NULL,
    kind       TEXT NOT NULL,
    exact      INTEGER NOT NULL,
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    line       INTEGER NOT NULL
);
CREATE INDEX edges_src ON edges(src_id);
CREATE INDEX edges_dst ON edges(dst_id);
CREATE INDEX edges_file ON edges(file_id);
CREATE INDEX edges_name ON edges(dst_name);
CREATE INDEX edges_bucket ON edges(dst_bucket);

-- Edges a language SDK's SCIP indexer resolved (`stone code native`). They sit
-- beside `edges` instead of in it, so a tree-sitter refresh never has to know
-- about them: a replaced file takes its symbols with it, and the cascades drop
-- every native edge that starts or ends there or whose site sits in it.
-- `name` is the identifier at the site, `line` its 1-based line. `kind` is
-- the kind of the reference site, or `dispatch` for an implementer pair.
-- `dst_id` is null for a site the indexer resolved outside the repo (the
-- standard library, a dependency): it adds no edge, it only hides the guesses.
CREATE TABLE native_edges (
    src_id  INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    dst_id  INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    kind    TEXT NOT NULL,
    name    TEXT NOT NULL,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    line    INTEGER NOT NULL
);
CREATE INDEX native_edges_site ON native_edges(src_id, kind, line);
CREATE INDEX native_edges_file ON native_edges(file_id);
CREATE INDEX native_edges_dst ON native_edges(dst_id);

-- The files a native run could have covered (`covered` = the indexer had the
-- file). A code file with no row, in a language that has rows, was added or
-- replaced since the run and needs another one.
CREATE TABLE native_files (
    file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    indexer TEXT NOT NULL,
    covered INTEGER NOT NULL
);

-- One row per run: the directory it covered (`''` is the repo root) and the
-- languages (comma separated) of the files it could have covered.
CREATE TABLE native_runs (
    indexer TEXT NOT NULL,
    base    TEXT NOT NULL,
    langs   TEXT NOT NULL,
    PRIMARY KEY (indexer, base)
) WITHOUT ROWID;

-- Every resolved edge with buckets expanded: a row into a bucket becomes one
-- inferred row per member, and `share` is the part of the site's weight that
-- member carries (1 for a direct edge, 1/k for a bucket of k members). Native
-- edges stand in for the tree-sitter edges they resolve (see `native_mask!`),
-- are exact and have `native` set. A guess whose site and target both sit in
-- files a native run covered is left out: the indexer saw both ends and did
-- not link them. A dispatch guess stays, since not every indexer writes
-- implementations (rust-analyzer writes none). Reads that want the flat graph use this; refresh writes and
-- compares the `edges` rows themselves.
CREATE VIEW edge_ends AS
    SELECT src_id, dst_id, dst_name, kind, exact, file_id, line, 1.0 AS share, 0 AS native
    FROM edges e WHERE dst_bucket IS NULL AND NOT "#,
    native_mask!("1"),
    r#" AND NOT (e.exact = 0 AND NOT "#,
    kept_guess!(),
    r#" AND "#,
    covered!("e.file_id"),
    r#" AND "#,
    covered!("(SELECT file_id FROM symbols WHERE id = e.dst_id)"),
    r#")
    UNION ALL
    SELECT e.src_id, m.symbol_id, d.qname, e.kind, 0, e.file_id, e.line, 1.0 / k.n, 0
    FROM edges e
    JOIN (SELECT bucket_id, count(*) AS n FROM bucket_members GROUP BY bucket_id) k
      ON k.bucket_id = e.dst_bucket
    JOIN bucket_members m ON m.bucket_id = e.dst_bucket
    JOIN symbols d ON d.id = m.symbol_id
    WHERE NOT "#,
    native_mask!("1"),
    r#" AND NOT (NOT "#,
    kept_guess!(),
    r#" AND "#,
    covered!("e.file_id"),
    r#" AND "#,
    covered!("d.file_id"),
    r#")
    UNION ALL
    SELECT n.src_id, n.dst_id, d.qname, n.kind, 1, n.file_id, n.line, 1.0, 1
    FROM native_edges n JOIN symbols d ON d.id = n.dst_id;

-- The `edges` rows (by rowid) that a native edge replaces. `hidden` is set
-- when only outside-the-repo sites do: the guess goes and nothing replaces it.
CREATE VIEW masked_edges AS
    SELECT e.rowid AS edge_rowid, e.exact AS exact, NOT "#,
    native_mask!("n.dst_id IS NOT NULL"),
    r#" AS hidden FROM edges e WHERE "#,
    native_mask!("1"),
    r#";

-- Symbols in search. Written and cleared by refresh next to the `symbols`
-- rows, keyed by symbol id as rowid; no triggers.
CREATE VIRTUAL TABLE symbols_fts USING fts5(
    name,
    qname,
    signature,
    doc,
    tokenize = "unicode61 tokenchars '_-'"
);

-- Import directives, in source order (`rowid`), so the resolver's input can be
-- rebuilt from rows. `global` and `glob` are stored as 0/1.
CREATE TABLE imports (
    file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    path      TEXT NOT NULL,
    alias     TEXT,
    is_glob   INTEGER NOT NULL,
    is_static INTEGER NOT NULL,
    is_global INTEGER NOT NULL,
    line      INTEGER NOT NULL
);
CREATE INDEX imports_file ON imports(file_id);

-- Grammar failures an extractor met in a file (`FileFacts::warnings`), in line order.
CREATE TABLE parse_warnings (
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    line    INTEGER NOT NULL,
    message TEXT NOT NULL
);
CREATE INDEX parse_warnings_file ON parse_warnings(file_id);

-- Project files (`*.csproj`, `Cargo.toml`, ...) are `files` rows with
-- lang = 'project'. `project_items` keeps their lists in order: kind is
-- `reference` (name, path), `package` (name) or `root` (path).
CREATE TABLE projects (
    file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    name    TEXT NOT NULL,
    root    TEXT NOT NULL
);
CREATE TABLE project_items (
    id      INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    kind    TEXT NOT NULL,
    name    TEXT NOT NULL,
    path    TEXT NOT NULL
);
CREATE INDEX project_items_file ON project_items(file_id);

CREATE TABLE modules (
    id       INTEGER PRIMARY KEY,
    project  TEXT NOT NULL,
    label    TEXT NOT NULL,
    file_id  INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE
);
CREATE INDEX modules_file ON modules(file_id);
CREATE INDEX modules_project ON modules(project);
"#
);
