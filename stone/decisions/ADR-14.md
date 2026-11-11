---
type: decision
title: 'Web UI: four spaces, state on every item, owner actions only'
status: accepted
created: 2026-11-04
generated: { by: claude-code, at: 2026-11-04 }
---

## Context

Backlog.md and Groma.md show state on each item. The Stone web UI made people read pages to learn the same facts. The 2026-11-04 release removed all editing while the PRD still promised it.

## Decision

- The UI has four spaces: Now, Product, System, Library. Each space has its own sidebar. Now is the home page.
- Every item shows its state at a glance: type, urgency, progress, owner, age. Empty values are left out.
- The look is Workbench: light cards with Geist type and one blue accent, plus a dark theme and phone views.
- The UI stays read-only except three owner actions: accept, send back with a comment, change task status. Each action is a file write in git.
- Text and field edits stay in the CLI and agents.
- The UI stays a Vite app. Fumadocs is not adopted. Its docs do not cover a client-only app with content loaded at run time. Its search, on-this-page and callout patterns may be copied.

## Consequences

The web app needs write routes for the three actions. Rejected: full editing, because it rebuilds what was removed. Rejected: read-only, because the review queue could not be acted on. Design: the Carvestone UI Design canvas, https://claude.ai/artifact/HxQ8gP4BF6G847yv1LLScy (private to the owner).
