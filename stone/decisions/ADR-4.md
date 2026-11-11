---
type: decision
title: Filename is the id; numbered files never move
status: accepted
created: 2026-10-23
generated: { by: claude-code, at: 2026-10-31 }
---

## Context

Backlog.md puts the title in the filename and moves files to `completed/`, which breaks paths and links. OKF uses the path as the concept id.

## Decision

- Numbered items are named `CS-12.md`, `M-3.md`, `ADR-7.md`; slug items by their slug.
- Status changes in place; archive is a status, not a folder.
- New numbers are the highest id seen across local and remote branches plus one, taken under the write lock.
- `stone check` finds collisions from parallel clones and `stone renumber` fixes them.
- Canonical case (`CS-7.md`), because NTFS ignores case.
- Amended 2026-10-31 (CS-135): a slug item (doc, plan, reference, architecture item) may be renamed with `stone rename`, which rewrites every link to it in one write. A numbered item is never renamed; a wrong one is removed or archived.

## Consequences

Links and paths of numbered items stay stable for the life of the item. A slug item keeps its history across a rename, and a renamed slug leaves no redirect, so outside notes that name the old slug stop matching.
