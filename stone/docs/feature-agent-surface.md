---
type: doc
title: Agent surface
status: draft
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: How an agent reads from and writes to Stone, through the CLI, search, guides and a read-only MCP server.
components: [arch/carvestone/stone/cli, arch/carvestone/stone/mcp, arch/carvestone/stone/engine]
links: [ADR-7, ADR-21]
---

Agents use the `stone` CLI. Stone answers in small results with ids and line ranges.

Status: live (read surface changes planned, ADR-21)

## What it does

- Every command takes `--json`. Writes take `--dry-run`.
- `stone search <words>` finds sections of docs, tasks, plans, decisions, architecture items and code symbols. It ranks by BM25 and boosts items linked to `--from` or `--component` ([[ADR-7]]).
- A hit costs about 40 tokens. `--budget N` caps the output. `stone get <id>#<anchor>` expands one section.
- `stone instructions [topic]` prints a guide: overview, plan, do, finish, debug or design.
- `stone instructions --write` adds the Stone block to `CLAUDE.md` or `AGENTS.md`.
- `stone mcp` serves `search`, `get`, `list` and `code` over MCP, for clients that cannot run a shell. Nothing turns it on by default.
- Writes always go through the CLI. An agent edit is stamped as the agent.
- Library lookups go to Docs MCP (`search_docs`), using the `libraries` of a component.

## Commands

| Command | Purpose |
| --- | --- |
| `stone search <words>` | Find sections by words, with `--from`, `--component`, `--type`, `--limit` and `--budget` |
| `stone get <id>[#<anchor>]` | Print an item or one section |
| `stone list` | List items, filtered by type, status, milestone or component |
| `stone code ...` | Query the code graph |
| `stone task next` | List the tasks that are ready to start |
| `stone commit-msg` | Print the trailers for the task in progress |
| `stone instructions [topic] [--write]` | Print or install the agent guide |
| `stone mcp` | Serve the read-only MCP tools |

## Limits

- ⚠ The MCP server is read-only. It has no write tool.
- ⚠ `--budget` is not accepted by every read command ([[CS-229]]).
- ⚠ An empty code answer does not say what Stone does not index, and `uses` finds nothing for Rust constants ([[CS-230]], [[CS-223]]).
- Output size is set by the caller. Stone does not decide it yet.

## Planned

[[ADR-21]] is proposed. It sets the direction: one linked store of docs, plans, tasks, architecture and code that an agent reads in one or two calls. The order of work is trust, then the map, then the brief.

| Part | Status | Task |
| --- | --- | --- |
| Wrong and empty answers fixed | planned | [[CS-230]], [[CS-223]] |
| `--budget` on every read command | planned | [[CS-229]] |
| Output tiers: Stone decides the size of an answer | planned | [[CS-232]] |
| Project map at the start of a session | planned | [[CS-231]] |
| Task brief and task pack: `stone brief` | planned | [[CS-202]] |
| Ready check before a task counts as ready | planned | [[CS-233]] |
| Agent guide with two pushes, two pulls and the writes | planned | [[CS-234]] |
| Freshness proof on each item or section | planned | no task yet |
| Tokens, turns and first-review result per task | planned | no task yet |
