---
type: component
title: carvestone-graph
description: Parses source files with tree-sitter into symbol and reference facts and resolves them into a code graph.
status: stable
technology: Rust
code: ['crates/graph/**']
uses: []
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
---

## Responsibilities

Turns file bytes into declarations, reference sites and project-file facts per language, then resolves references into exact or inferred edges.

## Interfaces

`extract(lang, path, src)` returns the facts for one file; the engine stores them in the index.

## Gotchas

A leaf crate: no SQLite and no engine. Extraction is a pure function of the bytes and never panics on broken input. `tree-sitter-language` is pinned to 0.1.5 in the lockfile because newer versions need rustc 1.90.
