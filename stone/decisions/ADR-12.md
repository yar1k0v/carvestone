---
type: decision
title: Precedent lookup ranks finished tasks by component and change shape
status: accepted
created: 2026-10-28
generated: { by: claude-code, at: 2026-11-07 }
links: [CS-111]
---

## Context

Agents on Freightline copy the pattern of a similar finished task before planning. Today only word search exists (CS-111).

## Decision

1. `stone precedents <task>` lists up to 8 finished tasks that made the same kind of change, best first, one line each: id, title, what matched, up to 4 changed files and "+n more". `--component <id>` and `--role <role>` search without a task; `--limit`, `--budget` and `--json` work as in `stone why`. A header line says what the lookup was based on.
2. Files come only from commits whose `Refs:` trailer names the task. Files under `stone/` and uncommitted files are left out.
3. Each changed file gets a component (from the architecture), a role (controller, route, entry point, test, otherwise the main symbol kind in the file) and whether it was added or modified. A task's shape is the set of pairs such as "added controller" or "modified test".
4. Score: +3 per shared component, +2 per shared shape pair, +1 for the same folder; word match only breaks ties; then newest first. Each line says which signals matched.
5. A planned task without commits takes its shape from its `components` and `paths` fields, else from a short word list ("endpoint" means controller, "test" means test) and component names in its text, else falls back to word search and says so.

Rejected: embeddings or vector search (needs a model or service, breaks the offline single binary, rankings cannot be explained); `stone search --status done --component X` (still matches words, cannot express "added a controller and its test"); a change-kind field filled in at done (depends on agents and goes stale).

## Consequences

Only tasks with trailer commits have files (41 of 99 done tasks here), so others match on words and rank lower. Commits that name more than 3 tasks are left out, because on this repo they gave every task the same files. Roles are read from today's code. It finds the same kind of files changed, not the same logic. No schema change; size medium.
