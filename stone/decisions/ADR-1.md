---
type: decision
title: Files in git are the truth; the SQLite index is disposable
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

Plans, docs and architecture must diff, merge and review like code, and stay usable without Carvestone.

## Decision

Every item is a markdown file with frontmatter under `stone/`. SQLite (FTS5, links, git cache) lives in `.stone/` and is rebuilt from the files at any time.

## Consequences

Git is the conflict layer. The index updates lazily by content hash, so it never has to be trusted.
