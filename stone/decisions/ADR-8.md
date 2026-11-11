---
type: decision
title: Architecture in our own agent-first format, backed by the code graph
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

Groma.md keeps relations in markdown tables and nests ids under a `groma:` key. That is fine for its map, but costly for agents to read and edit. A separate code graph (graphify) knows the real dependencies but nothing about components.

## Decision

- One file per C4 element in `stone/arch/`, nested by folder.
- Flat frontmatter: `type`, `title`, `description`, `technology`, `code` globs, `uses`, `libraries`, `status`.
- Fixed sections: `## Responsibilities`, `## Interfaces`, `## Gotchas`.
- The code graph supplies the real dependencies between components and reports drift against `uses`. Module clusters become draft components.
- Scans never edit curated fields or text, and running one twice changes no files.

## Consequences

Not Groma-compatible; `stone import groma` brings existing maps over.
