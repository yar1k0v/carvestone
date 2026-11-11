---
type: component
title: carvestone-cli
description: The stone binary; it parses arguments and renders results.
status: stable
technology: Rust
code: ['crates/cli/**']
uses: [arch/carvestone/stone/core, arch/carvestone/stone/engine, arch/carvestone/stone/mcp, arch/carvestone/stone/server]
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-05 }
---

## Responsibilities

Maps the command line to engine calls and prints text or `--json`.

## Interfaces

`stone [--root DIR] [--json] <command>`. `stone --help` is the full list: store (init, new, edit, link, task, renumber, rename, delete, import), reads (get, list, search, instructions), code and design (code, arch, scan, map), history (why, history, timeline, precedents), check, commit-msg, mcp and serve.

## Gotchas

Errors print one line starting with a cross mark and exit 1; usage mistakes exit 2.

## Uses

- [[arch/carvestone/stone/core]]: Parses ids and item fields
- [[arch/carvestone/stone/engine]]: Runs every command
- [[arch/carvestone/stone/mcp]]: Serves the read-only tools
- [[arch/carvestone/stone/server]]: Starts the browser server
