---
type: component
title: carvestone-engine
description: Every operation on a repo; the CLI and later adapters only parse and render.
status: stable
technology: Rust
code: ['crates/engine/**']
uses: [arch/carvestone/stone/core, arch/carvestone/stone/store, arch/carvestone/stone/search-index, arch/carvestone/stone/graph, arch/carvestone/stone/git]
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: claude-code, at: '2026-11-12T16:48:11+00:00' }
libraries: [ts-rs@12]
verified: [{ by: claude-code, at: '2026-11-12T16:48:12+00:00' }]
---

## Responsibilities

Holds every operation on a repo: items, tasks and their briefs, checks, search, the code graph and its answers, design views, git history, imports and multi-repo workspaces. It also builds the session map, writes `stone/index.md` and writes the OKF export.

## Interfaces

`Repo` is the one way in. `stone --help` shows what each operation does for a user. `stone code outline crates/engine/src/lib.rs` lists the public API.

## Gotchas

A writer takes the lock first and reads after it, never from an earlier snapshot. A dry run takes no lock.

## Uses

- [[arch/carvestone/stone/core]]: Reads and validates items
- [[arch/carvestone/stone/store]]: Reads and writes item files
- [[arch/carvestone/stone/search-index]]: Keeps search and graph rows
- [[arch/carvestone/stone/graph]]: Extracts and resolves code edges
- [[arch/carvestone/stone/git]]: Reads history and blame
