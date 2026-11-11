---
type: task
title: Dogfood M-1 on this repo
status: done
milestone: M-1
priority: medium
kind: chore
depends_on: [CS-6]
created: 2026-10-23
generated: { by: claude-code, at: 2026-10-25 }
closed: 2026-10-25
---

## Description

Write `stone/arch/` for this repo: a system for Carvestone and one component per crate, with `code` globs. Run `stone check` over the PRD, decisions, milestones and tasks, and fix what it finds. From here on, tasks are moved with `stone`, and commits carry `Refs:` trailers.

## Acceptance criteria

- [x] Every crate file is owned by exactly one component
- [x] `stone check` passes in CI
- [x] `stone instructions` gives a short guide to this workflow

## Outcome

arch/ holds the system and one component per crate (code globs cover every crate file, checked by ownership); CI runs stone check; stone instructions prints the workflow guide.
