---
type: decision
title: Restricted YAML frontmatter with a line-patch writer
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

CLI, UI and agents all edit items, and every edit must be a minimal diff.

## Decision

Frontmatter is a restricted YAML subset: flat scalars, inline lists and one-level inline maps; no anchors or multi-line values. Reads go through `serde_norway`. Writes patch only the changed lines, keep quoting, comments and line endings, and re-parse after each patch. Every write takes the lock, then re-reads, computes and saves; no write computes from a snapshot taken before the lock.

## Consequences

GitHub and Obsidian render the files. TOML was rejected for that reason, and `serde_yaml` (archived) and `serde_yml` (unsound) are not used.
