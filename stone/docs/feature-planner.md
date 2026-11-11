---
type: doc
title: Planner
status: stable
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: Tasks, a board, milestones, plans and decisions, kept as markdown files in git.
components: [arch/carvestone/stone/core, arch/carvestone/stone/store, arch/carvestone/stone/engine, arch/carvestone/stone/cli]
links: [ADR-4, ADR-5, ADR-6, ADR-14]
---

Stone keeps tasks, milestones, plans and decisions as one markdown file each, in `stone/`.

Status: live

## What it does

- Task: `stone/tasks/CS-12.md`. Sections: Description, Acceptance criteria, Plan, Notes, Outcome.
- Milestone `M-3`, plan `stone/plans/<slug>.md`, decision `ADR-7` (status, context, decision, consequences).
- A task moves by status. `stone task next` lists the tasks that are ready to start.
- The file name is the id. Numbered files never move, and archive is a status ([[ADR-4]]).
- A write patches only the changed frontmatter lines ([[ADR-5]]).
- Commit trailers tie commits to tasks, plans and decisions. A task's changed files come from its trailer commits and the working diff ([[ADR-6]]).
- `stone precedents <id>` lists finished tasks that made the same kind of change.
- The web UI shows a board with the state on every card. The owner can accept a task, send it back with a comment, or change its status. Each action is a file write in git ([[ADR-14]]).

## Commands

| Command | Purpose |
| --- | --- |
| `stone init --prefix TASK` | Create `stone/` and its config |
| `stone new task\|plan\|milestone\|decision` | Create an item |
| `stone edit <id>` with `--set`, `--section`, `--note`, `--check-ac <n>` | Change fields, a section, a note or a criterion |
| `stone task move <id> <status>` | Set a task's status |
| `stone task next` | List the tasks that are ready to start |
| `stone link <from> <to>...` | Add targets to an item's links |
| `stone list`, `stone get <id>` | Read items |
| `stone commit-msg` | Print the trailers for the task in progress |
| `stone precedents <id>` | Find finished tasks like this one |
| `stone check`, `stone renumber` | Validate the store, resolve id collisions |

## Limits

- ⚠ `stone edit <id> --section <heading>` on a heading with nested sections drops them without a warning (known defect, [[CS-228]]).
- The web UI changes no text or fields. Those edits stay in the CLI.
- Stone is local first. Teams share the store through git.

## Planned

- [[CS-228]]: `edit --section` warns before it drops nested sections.
- [[CS-222]]: the finish guide shows a repeated `--check-ac` that the CLI rejects.
- [[CS-188]]: `stale_after` accepts only a date.
- [[CS-235]]: timestamps carry a UTC offset.
- [[CS-233]]: a ready check before a task counts as ready.
