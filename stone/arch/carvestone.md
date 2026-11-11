---
type: system
title: Carvestone
description: Planning and knowledge store for humans and AI agents, with the CLI stone.
status: stable
technology: Rust
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-05 }
---

## Responsibilities

Keeps tasks, plans, decisions, docs and architecture as Markdown with frontmatter under `stone/`, and gives humans and agents one CLI to read, change and validate them.

## Interfaces

The `stone` binary: `stone --help` lists every command. `stone serve` opens the web UI, and `stone mcp` serves the read-only MCP tools.

## Gotchas

One binary holds the CLI, server and MCP server. The web UI ships inside it only in a `--features ui` build.
