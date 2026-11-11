---
type: component
title: carvestone-mcp
description: The optional read-only MCP stdio server behind `stone mcp`, for agent clients without a shell.
status: stable
technology: Rust
code: ['crates/mcp/**']
uses: [arch/carvestone/stone/engine]
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: 'human:yaroslav', at: 2026-10-26 }
---

## Responsibilities

Maps MCP tool calls to engine reads. Agents normally use the CLI; this server is only for clients that can't run a shell.

## Interfaces

The tools are search, get, list and code, with no writes and no resources. The `initialize` instructions say that reads happen here and writes go through the CLI.

## Gotchas

Stdout carries only JSON-RPC. Engine errors come back as the tool error text, never with a cross mark.

## Uses

- [[arch/carvestone/stone/engine]]: Answers every tool call
