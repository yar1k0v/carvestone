---
type: decision
title: Code graph from tree-sitter, derived and kept out of git
status: accepted
created: 2026-10-23
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

Agents need code structure (definitions, callers, dependencies, modules, impact) cheaply and up to date. Full compiler or LSP resolution per language is heavy; name matching alone is often wrong.

## Decision

- tree-sitter parsing for C#, Python, TypeScript/JavaScript and Rust, with a resolver per language that follows imports and scope.
- Edges are `exact` when resolved and `inferred` when matched by name. Every output shows which.
- Louvain clustering within project and folder boundaries gives modules.
- The graph lives in the SQLite index, rebuilt incrementally by file hash. It is never committed; curated knowledge goes in `stone/arch/`.
- Bar: `uses` finds at least 90% of IDE find-usages on a fixed Freightline sample. The full index builds in under 60 s, and in under 1 s per changed file.

## Consequences

No compiler or LSP needed. Precision is visible per edge, and more languages come as grammar plugins.
