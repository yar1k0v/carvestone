---
type: decision
title: One engine crate behind every adapter
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

The CLI, MCP and HTTP must behave the same.

## Decision

Crates:
- core (model);
- store (files, frontmatter, lock);
- index (SQLite);
- graph (tree-sitter code graph);
- git;
- engine (`Repo` and every operation);
- thin adapters: cli, mcp and server.

Each crate is created when its milestone starts.

## Consequences

Adapters only parse input and render output, so they can't drift.
