---
type: component
title: carvestone-git
status: stable
created: 2026-10-25
generated: { by: claude-code, at: 2026-10-25 }
description: 'Carvestone''s git wrapper: shells out to the git CLI and parses its output, with no SQLite and no writes to the repository.'
technology: Rust
code: [crates/git/**]
parent: arch/carvestone/stone
libraries: [thiserror@2]
---

## Responsibilities

Reads git history for the engine by running the `git` CLI: commit logs with trailers and changed files, one commit's patch of a file, a file at a commit, blame, working-tree changes, tracked and HEAD checks, shallow-clone detection and the hooks folder. It parses the output into plain structs and never writes to the repository.

## Interfaces

`Git::new(dir)` with `with_env`, `head`, `rev_list_reverse`, `is_ancestor`, `is_shallow`, `log`, `commits`, `file_log`, `added_lines`, `show_diff`, `show_file_diff`, `file_at`, `head_blob`, `blame`, `working_changes`, `is_tracked`, `differs_from_head` and `hooks_dir`. Parsers `parse_log`, `parse_blame`, `parse_added_lines` and `parse_name_status_z` take captured text. Types `Commit`, `FileChange`, `AddedLine`, `BlameLine` and `GitError` (Unavailable, TooOld, Command).

## Gotchas

Needs git 2.30 or newer; the version is checked once per process. Every call runs with `LC_ALL=C`, `core.quotepath=off`, no pager, `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE` removed, and a 30 s timeout that kills the process. A branch without commits gives an empty log, not an error. Commit fields are split by control characters, so a subject or trailer cannot break the parse. `file_log` with follow only tracks a rename through one path at a time. This crate holds no SQLite: the cache of commits lives in the index and the engine fills it.
