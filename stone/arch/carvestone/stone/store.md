---
type: component
title: carvestone-store
description: Frontmatter reader and line-patch writer, the stone/ layout, the write lock and id allocation.
status: stable
technology: Rust
code: ['crates/store/**']
uses: [arch/carvestone/stone/core]
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
---

## Responsibilities

Reads every item under `stone/`, patches frontmatter lines without touching the rest, takes the exclusive write lock, replaces files atomically and picks the next free number.

## Interfaces

`Store` (open, scan, lock, save, create, rename, next_number), `Document` (get, set, remove, to_text), `Config`.

## Gotchas

Every write needs a `WriteLock` and must read after taking it. Temp files have no `.md` suffix so a crash leftover is never scanned.
