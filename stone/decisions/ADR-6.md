---
type: decision
title: Git links come from trailers and the working diff, not hand-kept lists
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

Groma.md asks agents to log each modified file on the Backlog.md task by hand. That is easy to forget and goes stale.

## Decision

- Commits carry trailers: `Refs: CS-12`, `Plan: plans/x`, `Decision: ADR-7`.
- A task's changed files are the files of its trailer commits, plus the working diff while it is in progress.
- `stone why <path>[:line]` goes from blame to commits, to tasks and plans, to the owning component and its docs.
- History comes from the `git` CLI and is cached in the index by commit hash.
- `stone commit-msg` prints the trailer for the active task, and an opt-in hook checks for one.

## Consequences

Nothing to keep in sync by hand. Commits without trailers fall back to path → component → open tasks.
