---
type: decision
title: Replace Backlog.md, Groma.md, graphify and fumadocs; keep Docs MCP
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

Separate tools mean separate indexes, and none of them can link to the others.

## Decision

- Carvestone replaces Backlog.md, Groma.md, graphify and fumadocs, with importers for Backlog.md and Groma.md.
- Library docs stay in Docs MCP. `reference` items and component `libraries` point at them, and the agent guides send library lookups there.
- The MVP is local and single user. The file store, static export, the multi-repo workspace, team mode and embeddings come after it.

## Consequences

One store, index, CLI and MCP for project knowledge and code structure. The file store design (git folders, a local database file or a virtual file system) stays open until it is built.
