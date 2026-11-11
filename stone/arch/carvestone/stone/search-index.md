---
type: component
title: carvestone-index
description: Derived SQLite index with full-text search (milestone M-2).
status: stable
technology: Rust
code: ['crates/index/**']
uses: []
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
---

## Responsibilities

Will hold the rebuildable search index; it is a stub until M-2.

## Interfaces

None yet.

## Gotchas

The index lives under `.stone/` and is never committed.
