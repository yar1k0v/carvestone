---
type: doc
title: Code graph
status: stable
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: Symbols, dependencies, modules, impact and paths, derived from your source.
components: [arch/carvestone/stone/graph, arch/carvestone/stone/search-index, arch/carvestone/stone/engine]
links: [ADR-11, ADR-17]
---

Stone indexes your source into a graph of symbols and edges, and answers questions about it in a few lines.

Status: live

## What it does

- C#, Python, TypeScript/JavaScript, Rust and Dart work with no setup.
- Any other language works through a SCIP index that you build and list in `stone/config.yml`. For a language with no parser, the index creates the files and symbols ([[ADR-17]]).
- Nodes are projects, modules, files, types and functions, each with signature, doc comment and line range.
- Edges are contains, imports, calls, inherits or implements, and references.
- An edge is `exact` when it resolves through imports and scope. It is `inferred` when it matches by name only. Every answer shows which ([[ADR-11]]).
- The impact of a change: `stone code impact <symbol>` follows dependents transitively and lists the components, open tasks and docs on them.
- `stone code native` adds exact edges from the SDK indexers found on `PATH`. It never installs one.
- The graph lives in the index under `.stone/`, never in git. It refreshes on file change in `stone serve` and on each CLI call.
- Symbols appear in `stone search` and in `[[code:...]]` links.

## Commands

| Command | Purpose |
| --- | --- |
| `stone code find <name>` | Find where a symbol is defined |
| `stone code outline <file\|symbol>` | List what a file or type declares |
| `stone code uses <symbol\|file>` | List who calls, imports, inherits or references it |
| `stone code deps <symbol\|file>` | List what it imports and calls |
| `stone code impact <symbol\|file>` | List everything that depends on it (`--depth`, `--exact`) |
| `stone code path <a> <b>` | Find the shortest chain of edges between two symbols |
| `stone code explain <component>` | Show entry points, files, key symbols and dependencies |
| `stone code entries [<component>]` | Show where execution starts |
| `stone code native` | Add exact edges from SCIP indexers |
| `stone config add-scip-index <file>` | List a SCIP index in the config |
| `stone check --syntax` | List every parse error in the code |

## Limits

- ⚠ Config holds data only. It never runs a command, so a cloned repo cannot run code on your machine.
- ⚠ Go is proven on a small module only. Other SCIP languages are untested.
- ⚠ An `inferred` edge may be wrong. Add `--exact` to leave inferred edges out.
- ⚠ `stone code uses` and `stone code impact` find nothing for Rust constants ([[CS-223]]).
- An empty answer does not yet say what Stone does not index ([[CS-230]]).

## Planned

- [[CS-223]]: `uses` and `impact` find Rust constants.
- [[CS-230]]: an empty code answer says what Stone does not index.
- [[CS-204]]: `code outline` no longer fails with a foreign key error after a config change.
- [[CS-185]]: `code native` shows a readable error when scip-python fails.
