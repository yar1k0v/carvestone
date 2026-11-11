---
name: planning
description: Lead skill. Fills in a stone task so the Developer can finish it without making a decision. Runs after explore.
argument-hint: "[task-id]"
---

# planning (Lead)

The plan holds every decision, so the Developer only has to type. You write stone items, never code. Budget: 12 requests including explore.

## Find out

Send all lookups in one message, and go back only for what the answers reveal.

- Callers: `code uses <name> --limit 20` for each signature you change. Each caller's file joins `paths`.
- Reach: `code impact <name> --depth 2` for shared code.
- Earlier work: `why <path>:<line>` and `precedents <id>`. Name the commit or `path#Symbol` to copy.
- Library behavior: `mcp__docs__search_docs`. Name the doc page in the step.

## Write

Use `edit <id>` with `--set` for fields, and `--section <name>` for sections.

| Part | What it holds |
|---|---|
| `description` | 1–3 lines: what changes, for whom |
| `acceptance-criteria` | ``- [ ] `<command>` → <output that passes>``, narrowest crate first. Test filters state `N passed` |
| `plan` | Numbered steps, one action each. Name `path#Symbol`, never line numbers. For each seam: what it catches, what it misses, and the test choice (patch, add or none) |
| `components`, `paths` | Both are required. `paths` lists every file allowed to change |
| tag | `size-s` (up to 15 Developer requests) or `size-m` (up to 30). Bigger: propose a split to the owner, and create nothing |

Name any outside type a step uses once, as `path#Type`.

## Verify

1. `get <id>` shows every part above, and `check` adds no problems.
2. Read the plan as the Developer. Each step allows one reasonable result, and each name exists in the code or an earlier step.
3. Cut vague steps such as "add validation" or "handle errors".
4. Give each likely failure a test or a check.

Reply in up to 3 lines: the plan, its size, and what the owner must decide.

## Stop

- An open product or scope question: run `questions`, then plan. If `## Notes` already holds the owner's answer, use it.
- The work already exists in history: name the commit and propose done or archived.
