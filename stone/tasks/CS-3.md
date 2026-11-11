---
type: task
title: Frontmatter writer for the profile
status: done
milestone: M-1
priority: high
kind: feature
depends_on: [CS-2]
paths: [crates/store/src/frontmatter.rs]
created: 2026-10-23
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

Extend the existing line-patch writer (ADR-5) to the profile. Add inline lists of one-level inline maps, for `verified: [{ by: x, at: y }]`, and a bare-map form for a single entry, as OKF allows. Patches leave unknown keys, comments, quoting and line endings untouched.

## Acceptance criteria

- [x] `verified` appends as an inline list of maps; a single bare map is read too
- [x] Round-trip of a file with unknown keys and comments is byte-identical
- [x] Setting the same value twice gives no diff

## Outcome

Line-patch frontmatter writer: minimal diffs, CRLF kept, repeat writes give no change.
