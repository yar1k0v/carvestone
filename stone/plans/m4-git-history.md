---
type: plan
title: M-4 git and history requirements
description: Commit trailers, why, history, timeline, task changed files and staleness, as checkable behaviour. Serves the M-4 task list.
status: done
milestone: M-4
links: [M-4, ADR-1, ADR-6, ADR-10, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
---

## Summary

At the end of M-4 an agent goes from a line of code to the commits, tasks, plans, decisions, component and docs behind it (`stone why src/a.cs:42`), reads an item's versions with diffs (`stone history ADR-7`), scans what happened lately (`stone timeline --since 2026-10-01`), sees which files a task changed in `get`, and gets stale items flagged by `check` and `list --stale`. `stone commit-msg` prints the trailers for the active task and an opt-in hook checks them. Everything comes from the `git` CLI, cached in the index by commit hash. Server, UI and importers are out of scope.

## Background

- ADR-6: trailers `Refs: CS-12`, `Plan: plans/x`, `Decision: ADR-7`; a task's changed files are its trailer commits plus the working diff while in progress; history comes from the `git` CLI cached by commit hash; commits without trailers fall back to path, component, open tasks.
- Roadmap: `crates/git` is a leaf (core only, no SQLite); the cache is index schema v4 (`commits`, `commit_refs`, `blame`), refreshed from HEAD; adapters call engine only (ADR-10); risk is blame speed on Windows and large files, and shallow clones in CI.
- M-1 gave `owner` (path to component), M-2 `refresh` and `links`, M-3 component globs in drift and a `Warning` severity in `check`.
- Repo commits already use `Refs: CS-n` trailers (CLAUDE.md).

## Goals

- `why` answers from trailers when they exist and from structure when they do not, and says which it used.
- A warm `why path:line` stays under 1 s on a 5,000-line file with 10,000 commits (Linux).
- Nothing in the cache is trusted over git: a rewritten history rebuilds it.

## Users and scenarios

- An agent about to change `GetRoGateEventsQuery.cs:30` runs `stone why` and sees the task, plan and ADR behind the lines, the owning component and its docs. Failure path: the file is untracked; the error says so and suggests `git add`.
- A person asks "what happened to ADR-7": `stone history ADR-7` shows each commit with its diff. Failure path: the id has no commits yet (uncommitted file); it prints "ADR-7 has no commits yet" and exit 0.
- A dev commits: `git commit -F <(stone commit-msg)` style, or the hook rejects a message without `Refs:`. Failure path: two tasks are in progress; the command lists them and asks for `--task`.
- A CI job runs on a shallow clone: `why` works on what is there and prints that history is cut.

## Scope

- In: `crates/git`, schema v4 cache, `commit-msg` and the hook, `why`, `history`, `timeline`, changed files in `get` and `list --json`, staleness in `check` and `list`, MCP `why` and `history`.
- Out: writing to git (no commits, no `git config` changes except the opt-in hook file), remote hosting APIs, blame across repositories, UI (M-6), history of code symbols.

## Functional requirements

### FR-1 Git wrapper

`carvestone-git` shells out to `git -C <dir>` with `LC_ALL=C`, `-c core.quotepath=off`, `--no-pager` and never `--git-dir` guessing. It exposes `log` (sha, parents, author name, ISO date, subject, trailers, `--name-status -M` file changes), `blame` (porcelain lines for a range or file, `-w -M`), `show_file_diff(sha, path)`, `file_log(path, follow)`, `head`, `is_ancestor(a, b)`, `is_shallow`, `working_changes(paths)` (tracked changes and untracked, not ignored), `hooks_dir`. Parsers take captured output as text and are tested on canned samples, including renames, merge commits, multi-value trailers, non-ASCII paths and CRLF. A failed git call becomes `GitError::Command { args, stderr }`; no git or no repo is `GitError::Unavailable` with the sentence "git is not available in this folder".

Acceptance criteria:
- AC-1.1 Given canned `log` output with a rename, when parsed, then the commit has a file change `R` with old and new path.
- AC-1.2 Given `Refs: CS-1, CS-2` on one line and `Refs: CS-3` on another, when parsed, then the trailers hold three `Refs` values in order.
- AC-1.3 Given porcelain blame for 3 lines of 2 commits, when parsed, then each line maps to its sha and original line.
- AC-1.4 Given a folder that is not a repository, when any call runs, then the error is `GitError::Unavailable`.
- AC-1.5 Given a real temp repo built in the test, when `log` runs, then it equals what the canned parser produced for the same output.

### FR-2 Commit cache

Schema v4 (rebuild on bump) adds `commits(sha, date, author, subject, parents)`, `commit_files(sha, path, status, old_path)`, `commit_refs(sha, kind, ref_id)` with kind `task`, `plan`, `decision` and ids canonicalised when they resolve (as written otherwise), and `blame(path, content_hash, line_start, line_end, sha)`. Refresh appends `git log <old_head>..HEAD` and stores `meta.git_head` when done. A first fill or refill takes the whole history, no cap, in batches of 500 commits (M-2 `in_batches`, one transaction each) oldest first; `meta.git_filled` counts the commits stored from `git rev-list --reverse HEAD`, an interrupted fill resumes from that count, and `meta.git_head` is set only when the fill is complete. `why`, `history`, `timeline` and the staleness and changed-files rules complete the fill before answering; other reads may see a partial cache. The v4 bump uses the M-2 rebuild protocol. When the old head is not an ancestor of HEAD (rebase, force push, checkout of an unrelated branch) the commit tables are cleared and refilled. Blame is cached by the file's blob hash only for files equal to HEAD; a dirty file is blamed live and never cached. Invariants: refreshing twice writes nothing; the cache equals a clean rebuild of the whole history; a fill killed after any batch resumes to the same rows; outside git the tables stay empty and every git feature reports "git is not available in this folder".

Acceptance criteria:
- AC-2.1 Given 3 commits, when refresh runs, then `commits` has 3 rows; after a 4th commit, only that commit is added.
- AC-2.2 Given an amend of HEAD, when refresh runs, then the old sha is gone and the new one present.
- AC-2.3 Given a second refresh with no change, when it runs, then zero rows are written.
- AC-2.4 Given a trailer `Refs: ADR-7` pointing at an existing id, when cached, then `ref_id` is `decisions/ADR-7`; given `Refs: CS-999`, then `ref_id` is `CS-999` as written.
- AC-2.5 Given a dirty file, when blamed twice, then `blame` has no rows for it.
- AC-2.6 Given 1,200 commits and a fill stopped after its first batch, when refresh runs again, then `commits` has 1,200 rows equal to an uninterrupted fill and a Refs commit from the oldest batch is found by `get` of its task.

### FR-3 `stone commit-msg` and the hook

`stone commit-msg [--task ID]` prints the trailer lines for the active task, the only task with status `in-progress`: `Refs: CS-12`, plus `Plan: <plan>` when the task has `plan` and `Decision: ADR-n` for each decision in its `links`. With zero or several in-progress tasks and no `--task`, it exits 1 (`no task is in progress; pass --task CS-n` / `2 tasks are in progress (CS-3, CS-5); pass --task`). `--install-hook` writes `<hooks dir>/commit-msg`, a shell script starting `# stone commit-msg hook` that runs `stone commit-msg --check "$1"` and exits 0 when `stone` is not on PATH. `--uninstall-hook` removes only a file carrying that marker. `--check FILE` exits 1 with `✗ no Refs trailer; add one with \`stone commit-msg\`` unless the message has a `Refs:` trailer; messages starting `Merge `, `Revert `, `fixup!` or `squash!` pass. Install refuses to overwrite a hook without the marker.

Acceptance criteria:
- AC-3.1 Given CS-12 in progress with `plan: plans/x` and `links: [ADR-7]`, when `commit-msg` runs, then it prints `Refs: CS-12`, `Plan: plans/x`, `Decision: ADR-7`.
- AC-3.2 Given two tasks in progress, when run without `--task`, then exit 1 with the list.
- AC-3.3 Given `--install-hook` twice, when run, then the second prints "hook already installed" and the file is unchanged.
- AC-3.4 Given a foreign `commit-msg` hook, when installing, then exit 1 "a different commit-msg hook exists; remove it or add the check by hand".
- AC-3.5 Given `core.hooksPath` set, when installing, then the file lands in that folder.
- AC-3.6 Given a message file without `Refs:`, when `--check` runs, then exit 1 with the message; with `Refs: CS-1`, exit 0.

### FR-4 `stone why`

`stone why <path>[:line] [--json] [--budget N]` returns `{path, line, component, commits, tasks, plans, decisions, docs, fallback}`. With a line, blame gives its commit; without, the 5 newest commits touching the file (`git log -5`, follow renames). Commit trailers resolve to tasks, plans and decisions (`via: trailer`). The owner component comes from `code` globs; its docs are docs with it in `components` or a `[[code:...]]` link to the file; decisions linked from the component or the found tasks are added. If the commits yield no task, plan or decision, `fallback` is set to `component` and the owner's open tasks (`components` or `paths` cover the file, status not done) are listed with `via: component`; if there is no owner, `fallback` is `path` and tasks listing the file in `paths` are shown. Text output is one block per kind, one line each (`CS-12  done  Date-range pull  via trailer  3 commits`). Errors: untracked file (`<path> is not tracked by git; run \`git add\``), line past the end (`<path> has 90 lines`).

Acceptance criteria:
- AC-4.1 Given a line last changed by a commit with `Refs: CS-12`, when `why a.cs:5` runs, then CS-12 is listed with `via trailer`, with its plan and decisions.
- AC-4.2 Given no trailers, when `why a.cs:5` runs, then `fallback` is `component` and the component's open tasks are listed.
- AC-4.3 Given a file owned by no element and no trailers, when `why` runs, then `fallback` is `path`.
- AC-4.4 Given an untracked file, when run, then exit 1 with the sentence above and nothing else.
- AC-4.5 Given a line edited in the working tree and not committed, when `why` runs, then the commit list says `uncommitted change` for it and the rest comes from the file.
- AC-4.6 Given a renamed file, when `why` runs on the new name, then the pre-rename commits appear.
- AC-4.7 Given `--budget 150`, when run, then the cost is at most 150 and `truncated` is true in JSON.

### FR-5 `stone history`

`stone history <id>[#anchor] [--limit N] [--no-diff] [--json]` lists commits that touched the item's file, newest first (default limit 10), each as `<sha7>  <date>  <author>  <subject>` and, unless `--no-diff`, its diff of that file cut at 60 lines with `… N more lines`. With `#anchor` only commits whose diff touches that section's lines at that time are kept (computed from the diff hunks against the section's heading and range in that revision). Items without commits print "<id> has no commits yet" and exit 0.

Acceptance criteria:
- AC-5.1 Given ADR-7 with 3 commits, when `history ADR-7` runs, then 3 entries print newest first, each with a diff.
- AC-5.2 Given `--limit 2`, when run, then 2 entries.
- AC-5.3 Given a diff of 200 lines, when printed, then 60 lines plus `… 140 more lines`.
- AC-5.4 Given an uncommitted item, when run, then the message above and exit 0.
- AC-5.5 Given `history CS-2#outcome` where only one commit changed the outcome, when run, then one entry.
- AC-5.6 Given `--json`, when run, then each entry holds `sha`, `date`, `author`, `subject`, `refs` and `diff`.

### FR-6 `stone timeline`

`stone timeline [--since DATE] [--until DATE] [--component C] [--kind K]... [--limit N] [--json]` merges events newest first, default limit 30: `commit` (every cached commit), `task_done` (the commit that introduced `status: done` in a task file, found with `git log -G'^status: done$'` and the added line checked), `decision` (the commit that introduced `status: accepted`), `doc` (commits touching `stone/docs/**`). Each event: `{at, kind, id, title, sha}`. `--component` keeps events whose files fall under that component's globs or whose item lists it in `components`. `DATE` is `YYYY-MM-DD` or `Nd` (days back). Ties break by sha. Bad dates fail with "--since must be YYYY-MM-DD or Nd".

Acceptance criteria:
- AC-6.1 Given a task moved to done on day 3 and a commit on day 4, when `timeline` runs, then the commit precedes the `task_done` event.
- AC-6.2 Given `--since 2d`, when run on day 5, then no event older than day 3 appears.
- AC-6.3 Given `--kind task_done`, when run, then only those events.
- AC-6.4 Given `--component arch/x`, when run, then commits touching only other components are absent.
- AC-6.5 Given a task set done, then reopened, then done again, when run, then one `task_done` event per completion.

### FR-7 Changed files of a task

`stone get <task>` appends `Changed files:` after the body (before backlinks): paths from `commit_files` of every commit whose `Refs` names the task, deduplicated and sorted, each with status letter and short sha (`M  src/a.cs  1a2b3c4`); while the task is `in-progress`, working-tree changes outside `stone/` follow with `working`, limited to the task's `paths` and component globs when either is set. JSON adds `changed_files: [{path, status, source: "commit"|"working", sha?}]`; `list --json` rows for tasks add `changed_files` as the list of paths. A task with none prints no section. Deleted files keep status `D`.

Acceptance criteria:
- AC-7.1 Given two commits with `Refs: CS-4` touching `a.cs` and `b.cs`, when `get CS-4` runs, then both are listed once.
- AC-7.2 Given CS-4 in progress and `c.cs` modified in the working tree under its component, when `get` runs, then `c.cs` shows with source `working`.
- AC-7.3 Given CS-4 done, when the working tree has changes, then they do not show.
- AC-7.4 Given a file touched by three Refs commits, when listed, then it appears once with the newest sha.

### FR-8 Staleness

An item of type doc, reference, system, container or component is stale when `stale_after` is before today, or when at least N commits touched code under its components (for arch elements, their own `code` globs) after its reference date, the latest `verified.at`, else `generated.at`. N is `stale_commits` in `config.yml`, default 20. `check` prints `<path>:1: stale: <reason>` as a warning (exit code unchanged; `--strict` fails as in M-3). `list --stale` lists them, `get` prints `Stale: <reason>` after the status line, JSON adds `stale: {reason, commits}`. Dates compare as days.

Acceptance criteria:
- AC-8.1 Given `stale_after: 2026-01-01` and today later, when `check` runs, then a `stale` warning prints and exit is 0.
- AC-8.2 Given a doc verified on day 1 and 25 later commits under its component with `stale_commits` 20, when run, then it is stale with `commits: 25`.
- AC-8.3 Given 5 such commits, when run, then it is not stale.
- AC-8.4 Given no `verified`, when run, then `generated.at` is the reference date.
- AC-8.5 Given a task, when run, then it is never stale.
- AC-8.6 Given no git, when `check` runs, then the commit-count rule is skipped silently and `stale_after` still applies.

### FR-9 MCP tools

`why {path, line?, budget?}` and `history {id, limit?, diff?}` return the same JSON as the CLI; `tools/list` order ends `code, why, history` (nine tools). Errors are the engine sentences.

Acceptance criteria:
- AC-9.1 Given `why {path: "a.cs", line: 5}`, when called, then the JSON equals `stone why a.cs:5 --json`.
- AC-9.2 Given `history {id: "ADR-7", diff: false}`, when called, then entries carry no `diff`.

## Non-functional requirements

- Platforms: Windows, macOS, Linux; speed asserts on Linux only. Needs git 2.30 or newer; older prints "git 2.30 or newer is required".
- Warm `why` under 1 s as in Goals; first cache fill of 10,000 commits under 10 s; no network, no `git fetch`.
- Git calls are bounded: a 30 s timeout per call, and large outputs are streamed.
- Rust 1.89. No new dependencies: dates are parsed by own `YYYY-MM-DD` code; no libgit2.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| `git blame` slow on large files and on Windows | Slow `why` | Medium | Line-range blame (`-L`), whole-file blame never used for `why`; cache by blob hash |
| Shallow clone in CI | Missing history | High | Detect with `is_shallow`, print "history is cut at <sha>"; tests build their own repos |
| Rewritten history leaves a wrong cache | Wrong answers | Low | Ancestor check on HEAD, rebuild on mismatch |
| Date-dependent tests | Flaky CI | Medium | Fixed `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` and an injectable `today` |
| Working diff cannot be told apart per task | Over-reported files | Medium | Limited to `paths` and component globs when set |

## Alternatives considered

- libgit2 (`git2`): heavy to build on three OSes and a second view of the repo; the CLI is what users run.
- Hand-kept changed-file lists (Groma style): rejected by ADR-6.
- Blame the whole file for `why`: slow; a line range or recent log is enough.

## Open questions

None.

## Decision log

- 2026-10-25 The commit cache has no cap: the fill is resumable and runs in batches of 500, so "cache equals a clean rebuild" holds and old Refs commits are never missed - claude-code
- 2026-10-25 Tasks are split to one concern each (log parser, blame and misc, cache, why core, why fallbacks, ...) - claude-code
- 2026-10-25 `commit-msg` refuses to guess among several in-progress tasks - claude-code
- 2026-10-25 Stale applies to doc, reference and arch types, never tasks - claude-code
- 2026-10-25 `stale_commits` in config, default 20 - claude-code

## Tasks

### T1 Git crate: log, trailers, head and ancestry

Scope: `crates/git` with `log` and its parser, `head`, `is_ancestor`, `is_shallow`, errors (FR-1 part); about 250 lines.
Owned files: `crates/git/Cargo.toml`, `crates/git/src/lib.rs`, `crates/git/src/log.rs`, `crates/git/tests/parse_log.rs`, `Cargo.toml`.
Interfaces: `pub struct Git { dir: PathBuf }`; `Git::new(dir: &Path) -> Git`; `log(&self, range: Option<&str>, limit: Option<usize>, paths: &[&str]) -> Result<Vec<Commit>, GitError>`; `rev_list_reverse(&self) -> Result<Vec<String>, GitError>`; `head`, `is_ancestor`, `is_shallow`; `Commit { sha, parents, author, date, subject, trailers: Vec<(String, String)>, files: Vec<FileChange> }`; `FileChange { status: char, path: String, old_path: Option<String> }`; `GitError::{Unavailable, Command { args: String, stderr: String }}`.
Tests: `parses_rename` (AC-1.1), `parses_multi_trailers` (AC-1.2), `not_a_repo_is_unavailable` (AC-1.4), `real_repo_log_equals_canned` (AC-1.5).
Finish line: `cargo test -p carvestone-git parse_log` passes.
Depends on: M-3 done.
Risk: normal.

### T2 Git crate: blame, diffs, working changes, hooks dir

Scope: the remaining wrapper calls and their parsers (FR-1 rest); about 250 lines.
Owned files: `crates/git/src/blame.rs`, `crates/git/src/show.rs`, `crates/git/src/work.rs`, `crates/git/src/lib.rs`, `crates/git/tests/parse_blame.rs`, `crates/git/tests/real_repo.rs`.
Interfaces: `blame(&self, path: &str, lines: Option<(u32, u32)>) -> Result<Vec<BlameLine>, GitError>`; `show_file_diff(&self, sha: &str, path: &str) -> Result<String, GitError>`; `file_log(&self, path: &str, follow: bool, limit: usize) -> Result<Vec<Commit>, GitError>`; `working_changes(&self, paths: &[&str]) -> Result<Vec<FileChange>, GitError>`; `hooks_dir(&self) -> Result<PathBuf, GitError>`; `BlameLine { sha: String, orig_line: u32, final_line: u32, path: String }`.
Tests: `parses_porcelain_blame` (AC-1.3), `real_repo_blame_and_diff`, `working_changes_include_untracked`.
Finish line: `cargo test -p carvestone-git parse_blame` and `cargo test -p carvestone-git real_repo` pass.
Depends on: T1.
Risk: normal.

### T3 Commit cache

Scope: schema v4 and batched, resumable fill from HEAD (FR-2); about 300 lines.
Owned files: `crates/index/src/schema.rs`, `crates/engine/src/git_cache.rs`, `crates/engine/src/refresh.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/git_cache.rs`, `crates/engine/tests/support/testrepo.rs`.
Interfaces: `SCHEMA_VERSION = 4` (existing constant, value changes); `pub(crate) fn refresh_git(repo: &Repo, git: &Git) -> Result<GitReport, Error>` (batched); `pub fn cached_blame(repo: &Repo, path: &str, lines: (u32, u32)) -> Result<Vec<BlameLine>, Error>`; `pub(crate) fn ensure_filled(repo: &Repo) -> Result<(), Error>`; test helper `TestRepo::commit(msg, files, date)` (new); tables `commits`, `commit_files`, `commit_refs`, `blame`; `meta.git_head`, `meta.git_filled`.
Tests: `new_commits_appended` (AC-2.1), `amend_rebuilds` (AC-2.2), `second_refresh_writes_nothing` (AC-2.3), `trailer_ids_canonicalised` (AC-2.4), `dirty_file_not_cached` (AC-2.5), `interrupted_fill_resumes` (AC-2.6).
Finish line: `cargo test -p carvestone-engine git_cache` passes.
Depends on: T1, T2.
Risk: high (history rewrites, resumable fills).

### T4 `stone commit-msg` and hook

Scope: trailer printer, hook install and check (FR-3); about 250 lines.
Owned files: `crates/engine/src/commit_msg.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/commit_msg.rs`, `crates/cli/src/main.rs`, `crates/cli/tests/commit_msg.rs`.
Interfaces: `pub fn trailers_for(repo: &Repo, task: Option<&str>) -> Result<Vec<String>, Error>`; `pub fn install_hook(repo: &Repo) -> Result<HookState, Error>`; `HookState::{Installed, AlreadyInstalled}`; `pub fn uninstall_hook`; `pub fn check_message(text: &str) -> Result<(), Error>`; CLI `stone commit-msg [--task] [--install-hook | --uninstall-hook | --check FILE]`.
Tests: `prints_refs_plan_decision` (AC-3.1), `two_in_progress_errors` (AC-3.2), `install_twice_idempotent` (AC-3.3), `foreign_hook_refused` (AC-3.4), `hooks_path_respected` (AC-3.5), `check_requires_refs` (AC-3.6).
Finish line: `cargo test -p carvestone-engine commit_msg` and `cargo test -p carvestone-cli commit_msg` pass.
Depends on: T2.
Risk: normal.

### T5 `stone why`: blame to trailers

Scope: line or file to commits to tasks, plans, decisions, with errors and budget (FR-4 core); about 300 lines.
Owned files: `crates/engine/src/why.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/why_core.rs`, `crates/cli/src/main.rs`, `crates/cli/tests/why.rs`.
Interfaces: `pub fn why(repo: &Repo, path: &str, line: Option<u32>, budget: Option<usize>) -> Result<Why, Error>`; `Why { path, line, component: Option<String>, commits: Vec<CommitRef>, tasks: Vec<Related>, plans: Vec<Related>, decisions: Vec<Related>, docs: Vec<Related>, fallback: Option<String>, truncated: bool }`; `Related { id, status, title, via: String, commits: u32 }`; CLI `stone why <path>[:line]`.
Tests: `trailer_task_found` (AC-4.1), `untracked_errors` (AC-4.4), `uncommitted_line` (AC-4.5), `rename_follows` (AC-4.6), `budget_truncates` (AC-4.7).
Finish line: `cargo test -p carvestone-engine why_core` passes.
Depends on: T3.
Risk: high (blame edge cases).

### T6 `stone why`: component, docs and fallbacks

Scope: owner, docs, decisions, component and path fallbacks (FR-4 rest); about 200 lines.
Owned files: `crates/engine/src/why.rs`, `crates/engine/tests/why_fallback.rs`.
Interfaces: `fn fallback(repo: &Repo, path: &str, why: &mut Why) -> Result<(), Error>` (private); `owner` (existing).
Tests: `component_fallback` (AC-4.2), `path_fallback` (AC-4.3), `docs_and_decisions_of_component`.
Finish line: `cargo test -p carvestone-engine why_fallback` passes.
Depends on: T5.
Risk: normal.

### T7 `stone history`

Scope: engine module and CLI (FR-5); about 250 lines.
Owned files: `crates/engine/src/history.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/history.rs`, `crates/cli/src/main.rs`, `crates/cli/tests/history.rs`.
Interfaces: `pub fn history(repo: &Repo, id: &str, opts: &HistoryOpts) -> Result<History, Error>`; `HistoryOpts { limit: usize, diff: bool }`; `History { id: String, entries: Vec<HistoryEntry> }`; `HistoryEntry { sha, date, author, subject, refs: Vec<String>, diff: Option<String> }`.
Tests: `three_commits_newest_first` (AC-5.1), `limit_applies` (AC-5.2), `long_diff_cut` (AC-5.3), `uncommitted_item_message` (AC-5.4), `anchor_filters_commits` (AC-5.5), `json_fields` (AC-5.6).
Finish line: `cargo test -p carvestone-engine history` passes.
Depends on: T3.
Risk: normal.

### T8 `stone timeline`

Scope: event merge and CLI (FR-6); about 280 lines.
Owned files: `crates/engine/src/timeline.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/timeline.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn timeline(repo: &Repo, opts: &TimelineOpts) -> Result<Vec<Event>, Error>`; `TimelineOpts { since: Option<Date>, until: Option<Date>, component: Option<String>, kinds: Vec<EventKind>, limit: usize }`; `Event { at: String, kind: EventKind, id: String, title: String, sha: String }`; `EventKind::{Commit, TaskDone, Decision, Doc}`; `parse_date(&str, today: Date) -> Result<Date, Error>`.
Tests: `commit_before_task_done_same_range` (AC-6.1), `since_relative` (AC-6.2), `kind_filter` (AC-6.3), `component_filter` (AC-6.4), `reopened_task_two_events` (AC-6.5), `bad_since_message`.
Finish line: `cargo test -p carvestone-engine timeline` passes.
Depends on: T3.
Risk: normal.

### T9 Task changed files

Scope: changed files in `get` and `list --json` (FR-7); about 200 lines.
Owned files: `crates/engine/src/changed_files.rs`, `crates/engine/src/query.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/changed_files.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn changed_files(repo: &Repo, task: &Item) -> Result<Vec<ChangedFile>, Error>`; `ChangedFile { path: String, status: char, source: String, sha: Option<String> }`; `GetResult.changed_files` and `ListRow.changed_files` (fields added to existing structs).
Tests: `trailer_commit_files` (AC-7.1), `in_progress_adds_working` (AC-7.2), `done_hides_working` (AC-7.3), `dedupe_newest_sha` (AC-7.4).
Finish line: `cargo test -p carvestone-engine changed_files` passes.
Depends on: T3, T2.
Risk: normal.

### T10 Staleness

Scope: rule, `check` warning, `list --stale`, `get` line (FR-8); about 250 lines.
Owned files: `crates/engine/src/stale.rs`, `crates/engine/src/check.rs`, `crates/engine/src/query.rs`, `crates/engine/src/lib.rs`, `crates/core/src/config.rs`, `crates/engine/tests/stale.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn staleness(repo: &Repo, item: &Item, today: Date) -> Result<Option<Stale>, Error>`; `Stale { reason: String, commits: u32 }`; `Config.stale_commits: u32` (field added, default 20); `ListFilter.stale: bool` (field added).
Tests: `stale_after_passed` (AC-8.1), `commits_over_threshold` (AC-8.2), `under_threshold_fresh` (AC-8.3), `generated_is_fallback` (AC-8.4), `tasks_never_stale` (AC-8.5), `no_git_skips_commit_rule` (AC-8.6).
Finish line: `cargo test -p carvestone-engine stale` passes.
Depends on: T3.
Risk: normal.

### T11 MCP `why` and `history`

Scope: two tools over the engine (FR-9); about 100 lines.
Owned files: `crates/mcp/src/tools.rs`, `crates/mcp/tests/git_tools.rs`, `crates/engine/src/guide/do.md`.
Interfaces: handlers `why`, `history`; `WhyArgs { path: String, line: Option<u32>, budget: Option<usize> }`; `HistoryArgs { id: String, limit: Option<usize>, diff: Option<bool> }`.
Tests: `why_equals_cli_json` (AC-9.1), `history_without_diff` (AC-9.2), `tools_list_has_nine`.
Finish line: `cargo test -p carvestone-mcp git_tools` passes.
Depends on: T6, T7.
Risk: normal.
