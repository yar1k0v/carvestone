---
type: doc
title: Carvestone
status: stable
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: What Stone is, who it is for, and what joins its parts.
components: [arch/carvestone]
links: [ADR-16, ADR-21]
---

Your project's plan, docs, architecture and code graph, linked in one git repo, for people and AI agents.

## Who it is for

- AI agents that work on a software project. They read and write Stone through the `stone` CLI.
- People who steer those agents and read the same data in a local web UI (`stone serve`).
- Agents come first, people second ([[ADR-21]], proposed).
- Stone is one binary. It works offline and needs no account.

## Features

| Feature | Question it answers | Doc |
| --- | --- | --- |
| Planner | How do tasks, milestones, plans and decisions work? | [[docs/feature-planner]] |
| Docs | How are project docs stored, linked and served? | [[docs/feature-docs]] |
| Architecture | How does Stone hold and draw the design? | [[docs/feature-architecture]] |
| Code graph | What can Stone answer about the code, and how exact is it? | [[docs/feature-code-graph]] |
| Agent surface | How does an agent read from and write to Stone? | [[docs/feature-agent-surface]] |

## Links

The claim is the links between the parts ([[ADR-16]]). A task points at a code symbol, a doc shows the component it describes, and a commit names the task and decision behind it.

| Command | Answers |
| --- | --- |
| `stone why <path>[:line]` | Which commits, tasks, plans and decisions stand behind this code |
| `stone history <id>` | How an item changed, with diffs |
| `stone timeline` | What was done, decided and changed, newest first |
| `stone precedents <id>` | Which finished tasks made the same kind of change |
| `stone code impact <symbol>` | What may break, with the open tasks and docs on it |

Commit trailers (`Refs: CS-12`, `Plan: plans/x`, `Decision: ADR-7`) tie commits to items. `stone commit-msg` prints them.

## Principles

| Principle | In practice |
| --- | --- |
| Agents first, humans second | Formats and outputs are built for an agent that reads in few tokens and writes through the CLI. The web UI makes the same data comfortable for people |
| Curated knowledge in git, derived data in the index | Tasks, docs, decisions and architecture are files in git. The code graph, search index and git cache are derived, live in `.stone/` and can be rebuilt at any time |
| Minimal diffs | Writes patch only the changed frontmatter lines |
| OKF-compatible | Every item is an OKF concept with OKF field names where OKF has them. Unknown keys are kept |
| Stable ids | Files are named by id, never by title, and never move |
| Links over guessing | Explicit links and graph edges rank above text matches |
| One engine, many faces | CLI, MCP and web UI all call the same engine |

## Not goals

- Library docs. Docs MCP keeps doing that, and Stone links to it.
- A file store for images, decks and PDFs.
- A hosted multi-user server. Stone is local first, and teams share through git.
- Replacing git hosting, CI or Jira.
