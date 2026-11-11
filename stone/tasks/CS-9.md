---
type: task
title: Import apply, edit-detection state and CLI
status: done
created: 2026-10-25
generated: { by: claude-code, at: 2026-10-25 }
milestone: M-2
priority: medium
kind: feature
plan: plans/m2-search
depends_on: [CS-8]
paths: [crates/engine/src/import/mod.rs, crates/engine/src/lib.rs, crates/engine/tests/import_apply.rs, crates/cli/src/main.rs]
closed: 2026-10-25
---

## Description

Write step, `.stone/import-state.json`, dry-run and `stone import backlog`. Spec: plans/m2-search#fr-1-backlogmd-importer-tasks-and-docs, task T2. Lock first, then re-read and compute; state is written before the lock is released.

## Acceptance criteria

- [x] AC-1.2 second import prints `0 written, N unchanged`
- [x] AC-1.3 a task edited in stone is skipped, never overwritten
- [x] AC-1.4 `--dry-run` writes nothing
- [x] Missing state file skips an existing differing target
- [x] `cargo test -p carvestone-engine import_apply` passes

## Outcome

import::import_backlog/apply under the store lock with .stone/import-state.json (path to blake3 of the text last written): overwrite only untouched targets, skip edited ones, record unchanged ones; stone import backlog <dir> [--prefix] [--dry-run] [--json].
