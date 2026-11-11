---
type: task
title: Store layout, lock, atomic writes and id allocation
status: done
milestone: M-1
priority: high
kind: feature
depends_on: [CS-2, CS-3]
paths: [crates/store/src/]
created: 2026-10-23
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

- Load and save every item under `stone/`, with the folders from the PRD and the root `index.md` carrying `okf_version`. Read `config.yml`.
- Writes take an advisory lock on `.stone/lock` (`File::lock`) and replace the file atomically.
- New numbers come from the highest id across the working tree and all local and remote branches (`git for-each-ref` + `git ls-tree`), plus one, under the lock (ADR-4). Without git, the working tree alone.

## Acceptance criteria

- [x] Loading this repo's `stone/` gives every item with its path id
- [x] Two parallel writers never interleave a file or reuse an id
- [x] An id taken on another branch is not reused

## Outcome

Store layout, exclusive lock, atomic writes and id allocation across branches.
