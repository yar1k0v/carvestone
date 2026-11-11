---
type: plan
title: M-2 search requirements
description: Section-level search, backlinks and the Backlog.md task importer, as checkable behaviour. Serves the M-2 task list.
status: done
milestone: M-2
links: [M-2, ADR-1, ADR-7, ADR-5, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-07 }
---

## Summary

At the end of M-2 an agent finds the right section of any item with `stone search "query" --budget 200`, gets about 40-token hit lines with id, anchor and line range, and expands one with `stone get <id>#<anchor>`. `stone get` lists backlinks. The index is derived: one edit reindexes one file, a schema bump rebuilds silently. The Backlog.md task and doc importer comes first so a Freightline-shaped fixture exists in stone format. Code symbols, git, MCP and UI are out of scope.

## Background

- `crates/index/src/lib.rs` is a v1 stub (`chunks`, `chunks_fts`, `links`, `user_version`); nothing calls it. It is replaced, not extended.
- Headings, fences, code spans and links are scanned only by `crates/core/src/section.rs`; M-2 builds on it and writes no second parser.
- The roadmap fixes: one file `.stone/index.db`, WAL, `meta.schema_version`, mismatch deletes the file and rebuilds (ADR-1); refresh is `engine/src/refresh.rs` (mtime+size, then blake3, `BEGIN IMMEDIATE` transactions in bounded batches, see FR-4); the index is derived so refresh never takes the store lock; shared hit shape `{id, anchor, path, lines, kind, snippet, exactness?}`; `--budget` lives in the engine and counts chars/4 (ADR-7).
- `[[code:...]]` links exist from M-1 and `check` skips them. M-2 stores them as kind `code` unresolved; M-3 resolves them.

## Goals

- Recall@5 of at least 0.9 on the frozen `bench/search` query set, enforced in CI.
- A query under 50 ms on 5,000 sections: the target, printed by the bench; CI asserts only a loose p95 bound of 150 ms (Linux only).
- A second refresh writes nothing; an edit to one file touches only that file's rows.

## Users and scenarios

- An agent asks "how are ids allocated" and gets 5 lines, each `docs/prd#id-allocation  L40-48  doc  New ids come from the highest…`. Failure path: no match prints `no matches for "xyz"`, exit 0; a query of only operators (`"` or `*`) exits 2 saying the query has no words.
- An agent working on CS-12 runs `stone search jwt --from CS-12` and sections linked to CS-12 rank first. Failure path: `--from CS-99` exits 1 with "CS-99 not found".
- The lead dev edits a doc in an editor and runs `stone search` at once: the new text is found without any rebuild command. Failure path: the index file is corrupt or from another schema; it is deleted and rebuilt, and the query still answers.
- The lead dev imports a Backlog.md folder: `stone import backlog backlog/ --dry-run`, then for real, twice. Failure path: a task already edited in stone is skipped with a message, never overwritten.
- Two commands (a search and an edit) run at the same time: neither fails, neither sees a half-written index.

## Scope

- In: `stone import backlog` for tasks and docs; index schema v2; section units; content-hash refresh; link and backlink extraction; BM25 ranking with filters, boost and budget; `stone search`; backlinks in `get`; the search benchmark.
- Out: code symbols in search (M-3), history and `[[code:...]]` resolution (M-3), MCP (M-5), HTTP and watcher (M-6), the Backlog.md milestones and decisions importer and the Freightline switch (M-7), embeddings.

## Functional requirements

### FR-1 Backlog.md importer (tasks and docs)

`stone import backlog <dir> [--prefix P] [--dry-run]` reads `<dir>/tasks/task-<n> - <title>.md` (and `<dir>/completed/` and `<dir>/archive/tasks/`) and `<dir>/docs/**/*.md`. A task becomes `stone/tasks/<P>-<n>.md`: front matter `title`, `status` (`To Do` becomes `todo`, `In Progress` `in-progress`, `Done` `done`, any other value is lowercased with spaces as `-`), `priority`, `assignee`, `tags` from `labels`, `depends_on` from `dependencies` (ids rewritten to the prefix), `milestone` when it matches an existing `M-n`, `created` from `created_date`, `generated: { by: 'process:import-backlog', at: <updated_date or created_date> }`. Sections map: Description to `## Description`, Acceptance Criteria to `## Acceptance criteria`, Implementation Plan to `## Plan`, Implementation Notes to `## Notes`, Final Summary to `## Outcome`. A doc becomes `stone/docs/<same relative path>` with `type: doc`, title from the first heading or file name. `<P>` defaults to the task prefix in config. Numbers are kept. The importer keeps `.stone/import-state.json` (path to blake3 of the text it last wrote; local, never in an item). An existing target is overwritten only when its current text hashes to its stored value; a target whose text differs from both the stored hash and the planned text is skipped as edited in stone (also when the state file is missing); a target equal to the planned text is unchanged and its hash is recorded. The importer is a state function of its input: it takes the store lock first, then re-reads sources and targets, computes the whole output, writes only files whose text differs, and updates the state file before releasing the lock.

Acceptance criteria:
- AC-1.1 Given `backlog/tasks/task-7 - Fix login.md` with status `In Progress` and two labels, when the import runs with prefix `VC`, then `stone/tasks/VC-7.md` has `status: in-progress`, `tags: [a, b]` and the description section.
- AC-1.2 Given the same input, when the import runs a second time, then it prints `0 written, 12 unchanged` and no file changes.
- AC-1.3 Given `stone/tasks/VC-7.md` imported, then changed only by `stone task move VC-7 in-progress` (`generated` untouched), when the import runs again, then it prints `VC-7 skipped: edited in stone` and the file is unchanged.
- AC-1.4 Given `--dry-run`, when the import runs, then it prints each target path with `new`, `changed` or `unchanged` and writes nothing.
- AC-1.5 Given `dependencies: [task-3]`, when imported, then `depends_on: [VC-3]`; given the target VC-3 is not in the input, then the import ends with exit 1 and `task-3 not found in input` and writes nothing.
- AC-1.6 Given a golden fixture folder, when the import runs, then every output file equals its golden file byte for byte (LF endings).

### FR-2 Index schema v2

`.stone/index.db` holds `meta(key, value)` with `schema_version` and `fingerprint`; `files(id, path, hash, mtime_ns, size, item_id)`; `sections(id, file_id, item_id, anchor, title, level, line_start, line_end, kind, status, milestone, text)`; `fts` (FTS5 over title, heading, body, content-linked to `sections`); `links(src_item, src_anchor, dst_item, dst_anchor, kind, line)`. Paths are stored relative to the repo with `/`. On open, a missing `schema_version` or a value other than the compiled one triggers a rebuild; nothing is migrated. Rebuild protocol (also for every later bump, v3 and v4): take an exclusive lock on `.stone/index.lock`, re-read the version (another process may have rebuilt already; then just open), build a fresh db at `.stone/index.db.new`, then replace `index.db` by rename, retrying for 5 s when Windows reports it busy, and remove `-wal`/`-shm` leftovers of the old file after the swap. A file that is not a SQLite db is renamed to `index.db.bad` first. If the swap still fails: "cannot rebuild .stone/index.db; close other stone processes". Readers holding the old file keep working until they reopen. The crate API: `Index::open(path)`, `Index::open_versioned(path, version)` (tests), `Index::in_memory()`, `Index::schema_version()`.

Acceptance criteria:
- AC-2.1 Given a db at version 1, when `Index::open` runs, then the file is recreated at the current version and has no rows.
- AC-2.2 Given a db file with garbage bytes, when a query runs through the engine, then it succeeds after a silent rebuild.
- AC-2.3 Given the schema, when `sections` is read with `EXPLAIN QUERY PLAN` on `item_id`, then an index is used.
- AC-2.4 Given a version-1 db and 4 threads opening it at once, when all return, then no call failed, the db is at the current version and exactly one rebuild ran.
- AC-2.5 Given a reader holding the old db open during a bump, when the rebuild runs, then the swap succeeds (Windows: within the retry window) or fails with the sentence above and leaves the old file intact.

### FR-3 Section units

Each item yields a lead unit (anchor empty; title is the item title; text is the description, the tags and the text before the first heading) and one unit per heading found by the core scanners (headings in fences are ignored). A unit's text runs from its heading to the next heading of any level, so units never overlap; its anchor is the GitHub-style slug with `-1`, `-2` for repeats; its lines are the heading line to the line before the next heading. `kind` is the item type; `status` and `milestone` are copied from the item. Running the extraction twice on the same text gives the same units in the same order.

Acceptance criteria:
- AC-3.1 Given a doc with `# A`, `## B`, `## B`, when extracted, then the anchors are `a`, `b`, `b-1` and line ranges do not overlap.
- AC-3.2 Given `## X` inside a fenced block, when extracted, then no unit exists for X.
- AC-3.3 Given a task with no headings, when extracted, then one lead unit exists whose text holds the description.
- AC-3.4 Given CRLF text, when extracted, then line numbers equal those of the LF version.

### FR-4 Content-hash refresh

`engine::refresh::refresh(&Repo) -> Result<RefreshReport, Error>` with `RefreshReport { scanned, changed, removed }` lists every item file, skips a file whose mtime and size match its `files` row, hashes the others with blake3, and for a changed hash deletes that file's `sections`, `fts` and `links` rows and inserts the new ones, in `BEGIN IMMEDIATE` transactions with a 5 s busy timeout, at most `BATCH_FILES` (200) files per transaction, so a large first fill never holds the write lock past the timeout; each file's rows are replaced whole within one transaction, and the `files` rows are the resume cursor (an interrupted fill continues with files that do not match). Files that disappeared lose their rows. `Repo` reads (`get`, `list`, `search`) call it first. Invariants: a second run with no change writes nothing (zero rows touched); an edit to one file changes only that file's rows; two processes refreshing at once end with the same rows as one process; refresh never takes the store lock and never writes under `stone/`; a fill killed after any batch resumes and ends with the rows of an uninterrupted run; a reader during a fill sees every file either fully old or fully new. Later milestones' graph and git fills use the same batch helper.

Acceptance criteria:
- AC-4.1 Given a refreshed repo, when refresh runs again, then `changed` is 0 and the db file's page count and `PRAGMA data_version` writes did not increase.
- AC-4.2 Given 3 items, when one file is edited, then `changed` is 1 and the other two files' `sections` rowids are unchanged.
- AC-4.3 Given a file edited to the same size with a changed mtime, when refresh runs, then it is hashed and its rows are replaced.
- AC-4.4 Given 4 threads calling refresh on one repo, when all return, then no call failed and the row counts equal a single run's.
- AC-4.5 Given a deleted file, when refresh runs, then its sections and links are gone and `removed` is 1.
- AC-4.7 Given 1,000 items and `BATCH_FILES` set to 100 by the test, when refresh runs while a reader queries, then every query returns within 1 s, and rows equal an unbatched run.
- AC-4.8 Given a refresh stopped after its second batch, when refresh runs again, then the rows equal an uninterrupted run and `changed` counts only the remaining files.
- AC-4.6 Given an unwritable `.stone/` (read-only), when `stone search` runs, then it exits 1 with "cannot write .stone/index.db; check permissions".

### FR-5 Links and backlinks

Refresh extracts, per section, the links from the relation fields (`links`, `depends_on`, `milestone`, `plan`, `components`, `supersedes`, `parent`, `uses`; kind = field name), `[[id]]`, `[[id#anchor]]` (kind `wikilink`), markdown links to `.md` files in `stone/` (kind `md`), and `[[code:...]]` (kind `code`, destination kept as text). Targets are stored as canonical ids; an unresolved target is stored as written and counted by `check` as before. `stone get <id>` lists backlinks after the file text under `Backlinks:`, one line each: `<src id>[#anchor]  L<line>  <kind>`; `--json` adds `backlinks: [{id, anchor, path, line, kind}]`, sorted by id, anchor, line. With `#section` only links into that anchor show.

Acceptance criteria:
- AC-5.1 Given CS-2 with `depends_on: [CS-1]`, when `stone get CS-1 --json` runs, then `backlinks` holds `{id: "tasks/CS-2", kind: "depends_on"}`.
- AC-5.2 Given a doc containing `[[ADR-3#decision]]` on line 12, when `stone get ADR-3#decision` runs, then the backlink line reads `docs/x  L12  wikilink`.
- AC-5.3 Given a link inside a code fence, when extracted, then no link row exists.
- AC-5.4 Given an item without backlinks, when `get` runs in text mode, then no `Backlinks:` line prints.

### FR-6 Query and ranking

`engine::search::search(&Repo, &SearchQuery) -> Result<SearchResult, Error>`. Query text becomes FTS terms (words quoted, AND; if that gives nothing, OR; the last word also matches as a prefix). Rank is `bm25(fts, 10.0, 5.0, 1.0)` (title, heading, body) adjusted by a boost: sections of items directly linked (either direction) to `--from <id>` rank ahead by a factor of 1.5, and items listing `--component C` in `components`, or living under it, by 1.3. Filters: `--type`, `--status`, `--milestone`, `--component`. `--limit N` (default 10). `--budget N` stops adding hits when the sum of their text lines (chars/4, rounded up) would exceed N, but always returns at least one hit. A hit is `{id, anchor, path, lines: [start, end], kind, snippet}`; the text line is `<id>[#anchor]  L<start>-<end>  <kind>  <snippet>` with the snippet cut to 100 chars and a `…`. Results are deterministic: ties break by id, then line.

Acceptance criteria:
- AC-6.1 Given two sections matching equally, when one belongs to an item linked from `--from CS-1`, then it ranks first.
- AC-6.2 Given a heading match and a body-only match of the same word, when searched, then the heading match ranks first.
- AC-6.3 Given 10 matching sections whose lines each cost 40 (chars/4) and `--budget 200`, when searched, then exactly 5 hits return and `truncated` is true.
- AC-6.4 Given `--budget 1`, when anything matches, then exactly one hit is returned.
- AC-6.5 Given `--type task --status todo`, when searched, then only todo tasks appear.
- AC-6.6 Given the query `"` , when searched, then the error is "query has no words; give at least one search term".
- AC-6.7 Given the same repo state, when the same query runs twice, then the hit lists are equal.

### FR-7 `stone search`

`stone search <query>... [--from ID] [--component C] [--type T] [--status S] [--milestone M] [--limit N] [--budget N] [--json]` prints the FR-6 lines, or `no matches for "<query>"` (exit 0). `--json` prints `{query, hits: [...], truncated}` where `truncated` is true when the budget cut hits. Errors: `✗ <sentence>`, exit 1; usage errors exit 2.

Acceptance criteria:
- AC-7.1 Given this repo, when `stone search "minimal diff" --budget 200` runs, then every line matches `^\S+(#\S+)?  L\d+-\d+  \w+  .+$`.
- AC-7.2 Given `--json`, when run, then the output parses and each hit has `id`, `anchor`, `path`, `lines`, `kind`, `snippet`.
- AC-7.3 Given `--from CS-99`, when run, then the output is `✗ CS-99 not found` and the exit code is 1.

### FR-8 Benchmark

`bench/search/` holds a frozen synthetic store (`stone/` of about 60 items modeled on Freightline's shapes: integrations, auth, billing, infra) and `queries.yml` (`- q: ..., expect: [id#anchor, ...]`, at least 40 queries). `engine/tests/bench_search.rs` builds the index and asserts recall@5 (the share of queries with at least one expected section in the top 5) is at least 0.9, prints tokens per answer, and, on Linux only, generates 5,000 sections in a temp repo, prints the p95 query time against the 50 ms target and asserts only the loose bound of 150 ms. Freightline numbers are measured locally and recorded in the milestone file.

Acceptance criteria:
- AC-8.1 Given the fixture, when `cargo test -p carvestone-engine bench_search` runs, then recall@5 is at least 0.9.
- AC-8.2 Given a query whose expected section is removed from the fixture, when the test runs, then it fails and names the query.

## Non-functional requirements

- Platforms: Windows, macOS, Linux; the latency assert is Linux only.
- First refresh on a 5,000-section store under 3 s; a no-change refresh under 100 ms.
- Offline. Rust 1.89. New dependencies: `blake3`; rusqlite stays bundled with FTS5. `serde_norway` already reads the importer's input.
- Errors: engine returns plain sentences naming the next step; only the CLI adds `✗`.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Refresh racing with another refresh or an edit | Missed or duplicated rows | Medium | One `BEGIN IMMEDIATE` transaction, re-check hash inside it, thread and process tests |
| mtime+size fast path misses an edit with equal size and restored mtime | Stale hit | Low | Documented; any real edit moves mtime |
| Backlog.md format variants | Importer drops fields | Medium | Golden files; unknown keys listed in the output, never silently lost |
| FTS tokenizing hurts identifiers (`CS-12`, `snake_case`) | Misses | Medium | `unicode61` with `tokenchars '_-'`; benchmark queries include identifiers |

## Alternatives considered

- Migrating the schema between versions: rejected by ADR-1; delete and rebuild is simpler and safe.
- Overlapping units (a section includes its subsections): double-counts text in BM25; `get #section` already returns the subtree.
- Taking the store lock in refresh: would block reads behind writes; the index is derived.

## Open questions

None.

## Decision log

- 2026-10-25 Units do not overlap; `get #section` still returns the subtree - claude-code
- 2026-10-25 The importer skips targets edited in stone, so a re-import never destroys work - claude-code
- 2026-10-25 A budget always returns at least one hit - claude-code
- 2026-10-25 Search with no matches exits 0 - claude-code
- 2026-10-25 Importer edit detection uses a stored hash of the last written text in `.stone/import-state.json`, not `generated.by`, because status moves and link edits leave `generated` alone - claude-code
- 2026-10-25 Refresh commits in batches of 200 files; the `files` table is the resume cursor - claude-code
- 2026-10-25 Index rebuild is an exclusive lock file plus build-then-rename; the old file is never deleted while open - claude-code
- 2026-10-25 `SearchResult` is `{query, hits, truncated}` everywhere (CLI, MCP, HTTP) - claude-code
- 2026-10-25 50 ms is a reported target; CI asserts 150 ms - claude-code

## Tasks

### T1 Backlog.md task and doc mapping

Scope: parse and map one source folder to planned files, with golden tests (FR-1 mapping); about 300 lines.
Owned files: `crates/engine/src/import/backlog.rs`, `crates/engine/tests/import_backlog.rs`, `crates/engine/tests/golden/backlog/**`.
Interfaces: `pub fn plan_backlog(repo: &Repo, dir: &Path, prefix: Option<&str>) -> Result<ImportPlan, Error>`; `ImportPlan { files: Vec<PlannedFile>, gone: Vec<String> }`; `PlannedFile { path: String, text: String, importer: &'static str }`.
Tests: `maps_status_labels_sections` (AC-1.1), `unknown_dependency_fails_before_planning` (AC-1.5), `golden_files_match` (AC-1.6). Invariant: planning is pure; the same input gives the same plan.
Finish line: `cargo test -p carvestone-engine import_backlog` passes.
Depends on: none.
Risk: normal.

### T2 Import apply, edit-detection state and CLI

Scope: write step, `.stone/import-state.json`, dry-run, `stone import backlog` (FR-1 run rules); about 280 lines.
Owned files: `crates/engine/src/import/mod.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/import_apply.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn import_backlog(repo: &Repo, dir: &Path, opts: &ImportOpts) -> Result<ImportReport, Error>`; `pub fn apply(repo: &Repo, plan: ImportPlan, opts: &ImportOpts) -> Result<ImportReport, Error>`; `ImportOpts { prefix: Option<String>, dry_run: bool }`; `ImportReport { written: usize, unchanged: usize, skipped: Vec<String>, entries: Vec<(String, ImportState)> }`; `ImportState { New, Changed, Unchanged, Skipped }`; CLI `stone import backlog <dir> [--prefix P] [--dry-run]` (new).
Tests: `import_twice_is_noop` (AC-1.2), `status_move_makes_target_skipped` (AC-1.3), `dry_run_writes_nothing` (AC-1.4), `missing_state_file_skips_existing_differing_target`. Invariants: lock first, then re-read and compute; running twice gives the same bytes; state is written before the lock is released.
Finish line: `cargo test -p carvestone-engine import_apply` passes.
Depends on: T1.
Risk: normal.

### T3 Index schema v2 and rebuild protocol

Scope: replace the stub with the v2 schema and the lock-file rebuild (FR-2); about 300 lines.
Owned files: `crates/index/src/lib.rs`, `crates/index/src/schema.rs`, `crates/index/src/rebuild.rs`, `crates/index/tests/schema.rs`, `crates/index/Cargo.toml`.
Interfaces: `pub const SCHEMA_VERSION: i64 = 2` (existing, value changes); `Index::open(path: &Path) -> Result<Index, IndexError>` (existing); `Index::open_versioned(path: &Path, version: i64) -> Result<Index, IndexError>`; `Index::in_memory()` (existing); `Index::schema_version(&self) -> Result<i64, IndexError>`; `Index::conn(&self) -> &Connection`; tables `meta`, `files`, `sections`, `fts`, `links` as in FR-2. `put_chunk`, `search` and `Hit` are removed.
Tests: `old_version_is_rebuilt` (AC-2.1), `garbage_file_is_rebuilt` (AC-2.2), `item_id_index_used` (AC-2.3), `four_threads_open_old_db_one_rebuild` (AC-2.4), `reader_during_bump_keeps_old_file_or_clear_error` (AC-2.5), `open_twice_keeps_rows`. Invariant: the old file is never deleted while another handle may use it.
Finish line: `cargo test -p carvestone-index` passes.
Depends on: none.
Risk: high (concurrent rebuild, Windows rename).

### T4 Section units

Scope: pure function from item text to units (FR-3); about 150 lines.
Owned files: `crates/engine/src/units.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/units.rs`.
Interfaces: `pub struct Unit { pub anchor: String, pub title: String, pub level: u8, pub line_start: u32, pub line_end: u32, pub text: String }`; `pub fn units_of(item: &Item, text: &str) -> Vec<Unit>`; uses `carvestone_core::section` scanners (existing).
Tests: `duplicate_headings_get_suffixes` (AC-3.1), `fenced_heading_ignored` (AC-3.2), `headingless_task_has_lead_unit` (AC-3.3), `crlf_lines_equal_lf` (AC-3.4), `extract_twice_equal`.
Finish line: `cargo test -p carvestone-engine units` passes.
Depends on: none.
Risk: normal.

### T5 Content-hash refresh

Scope: batched refresh driver writing files, sections and fts rows (FR-4, links excluded until T6); about 300 lines.
Owned files: `crates/engine/src/refresh.rs`, `crates/engine/src/repo.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/refresh.rs`, `crates/engine/Cargo.toml`.
Interfaces: `pub fn refresh(repo: &Repo) -> Result<RefreshReport, Error>`; `RefreshReport { scanned: usize, changed: usize, removed: usize }`; `pub(crate) const BATCH_FILES: usize = 200`; `pub(crate) fn in_batches<T>(items: Vec<T>, size: usize, f: impl FnMut(&Transaction, &[T]) -> Result<(), Error>) -> Result<(), Error>` (shared by later milestones); `Repo::index(&self) -> Result<Index, Error>` (new); `Repo::get`, `list` call refresh first.
Tests: `second_refresh_writes_nothing` (AC-4.1), `one_edit_reindexes_one_file` (AC-4.2), `same_size_edit_rehashed` (AC-4.3), `four_threads_refresh` (AC-4.4), `deleted_file_loses_rows` (AC-4.5), `readonly_index_dir_message` (AC-4.6), `batched_fill_reader_stays_responsive` (AC-4.7), `interrupted_fill_resumes` (AC-4.8). Invariants: running twice gives the same rows; the hash is re-read inside the transaction; no write under `stone/`.
Finish line: `cargo test -p carvestone-engine refresh` passes.
Depends on: T3, T4.
Risk: high (concurrency, batches, the two-process case, mtime fast path).

### T6 Links and backlinks

Scope: link extraction in refresh and backlinks in `get` (FR-5); about 250 lines.
Owned files: `crates/engine/src/links.rs`, `crates/engine/src/refresh.rs`, `crates/engine/src/query.rs`, `crates/engine/src/lib.rs`, `crates/cli/src/main.rs` (get output), `crates/engine/tests/backlinks.rs`.
Interfaces: `pub fn links_of(item: &Item, text: &str) -> Vec<LinkRow>`; `LinkRow { src_anchor: String, dst_item: String, dst_anchor: Option<String>, kind: String, line: u32 }`; `GetResult.backlinks: Vec<Backlink>` (existing struct gains the field); `Backlink { id: String, anchor: Option<String>, path: String, line: u32, kind: String }`.
Tests: `depends_on_backlink` (AC-5.1), `wikilink_anchor_backlink` (AC-5.2), `fenced_link_ignored` (AC-5.3), `no_backlinks_no_line` (AC-5.4), `relink_replaces_rows` (refreshing after an edit leaves no stale link rows).
Finish line: `cargo test -p carvestone-engine backlinks` passes.
Depends on: T5.
Risk: normal.

### T7 Query, ranking and budget

Scope: `engine::search` (FR-6); about 300 lines.
Owned files: `crates/engine/src/search.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/search.rs`.
Interfaces: `pub fn search(repo: &Repo, q: &SearchQuery) -> Result<SearchResult, Error>`; `SearchQuery { text: String, from: Option<String>, component: Option<String>, kind: Option<String>, status: Option<String>, milestone: Option<String>, limit: usize, budget: Option<usize> }`; `SearchResult { query: String, hits: Vec<Hit>, truncated: bool }`; `Hit { id: String, anchor: Option<String>, path: String, lines: [u32; 2], kind: String, snippet: String, exactness: Option<String> }` (serde-derived, shared by `--json`, MCP and HTTP).
Tests: `from_boost_ranks_linked_first` (AC-6.1), `heading_beats_body` (AC-6.2), `budget_caps_cost` (AC-6.3), `budget_one_returns_one` (AC-6.4), `filters_apply` (AC-6.5), `no_words_query_errors` (AC-6.6), `repeat_query_equal` (AC-6.7).
Finish line: `cargo test -p carvestone-engine search` passes.
Depends on: T5, T6.
Risk: normal.

### T8 `stone search` command

Scope: CLI wiring and rendering (FR-7); about 120 lines.
Owned files: `crates/cli/src/main.rs`, `crates/cli/tests/search.rs`, `crates/cli/src/instructions.md`.
Interfaces: `Command::Search(SearchArgs)` (new) with the flags in FR-7; output line format from FR-6; `instructions.md` names `search`.
Tests: `search_lines_match_format` (AC-7.1), `search_json_shape` (AC-7.2), `unknown_from_exits_1` (AC-7.3), `no_match_exit_0`.
Finish line: `cargo test -p carvestone-cli search` passes.
Depends on: T7.
Risk: normal.

### T9 Benchmark harness

Scope: fixture store, query set and the two tests (FR-8); about 300 lines of Rust plus fixture data.
Owned files: `bench/search/stone/**`, `bench/search/queries.yml`, `crates/engine/tests/bench_search.rs`, `.github/workflows/ci.yml` (run the bench test by name).
Interfaces: none new; uses `search` and `refresh` (existing after T7).
Tests: `recall_at_5_meets_bar` (AC-8.1), `missing_expected_section_fails` (AC-8.2), `latency_5000_sections` (Linux only, prints p95 against 50 ms, asserts 150 ms).
Finish line: `cargo test -p carvestone-engine bench_search` passes with recall printed.
Depends on: T7, T2.
Risk: normal.
