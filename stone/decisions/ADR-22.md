---
type: decision
title: Schema and infra files parse with pure-Rust crates; the agent hook only advises
status: proposed
created: 2026-11-12T20:27:31+00:00
generated: { by: claude-code, at: '2026-11-12T20:27:31+00:00' }
links: [ADR-17, ADR-11]
---

## Context

Stone's code graph has no SQL schemas, Terraform or config files, and nothing steers an agent from a raw Read or Grep to stone. Each new tree-sitter grammar costs 0.6 to 5.4 MiB in the release binary, and WebAssembly grammars were dropped at +4.6 MiB (CS-282).

## Decision

- SQL: DDL in `.sql` files only (tables, views, functions, foreign keys), parsed with `sqlparser`. Stone reads no live database.
- Terraform: resources, modules and an edge from each module to its source, parsed with `hcl-edit`.
- YAML and JSON: top-level keys become symbols with line ranges. They get no edges.
- `stone hook`: reads the Claude Code PreToolUse JSON for Read and Grep and adds a note that names the stone command that answers it. It never blocks, and stone never edits `settings.json`.
- No generic tree-sitter walker for new languages. New languages come from SCIP (ADR-17).

## Consequences

Each new parser's size cost is measured on the release binary before it lands. Config files stay data: no command runs from them.
