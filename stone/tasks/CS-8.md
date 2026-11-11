---
type: task
title: Backlog.md task and doc mapping
status: done
created: 2026-10-25
generated: { by: claude-code, at: 2026-10-25 }
milestone: M-2
priority: medium
kind: feature
plan: plans/m2-search
paths: [crates/engine/src/import/backlog.rs, crates/engine/tests/import_backlog.rs, crates/engine/tests/golden/backlog/]
closed: 2026-10-25
---

## Description

Map one Backlog.md folder to planned files with golden tests. Spec: plans/m2-search#fr-1-backlogmd-importer-tasks-and-docs, task T1. Planning is pure: the same input gives the same plan.

## Acceptance criteria

- [x] AC-1.1 status, labels and description map to the stone task
- [x] AC-1.5 unknown dependency fails before planning
- [x] AC-1.6 golden files match byte for byte
- [x] `cargo test -p carvestone-engine import_backlog` passes

## Outcome

plan_backlog maps a Backlog.md folder (tasks, completed, archive, docs) to planned stone files; pure and sorted. Unknown front-matter keys and unmatched milestones come back as notes. Golden files match byte for byte.
