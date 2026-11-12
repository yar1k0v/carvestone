---
name: planning
description: Fills in a stone task so the developer can finish it without a decision. Use when the Lead has an explored task that is not yet ready.
argument-hint: "[task-id]"
---

# planning

The plan holds every decision, so the developer only types. Write stone items, never code. Budget: 12 requests, explore included.

## Find out

Send all lookups in one message. Go back only for what the answers reveal.

1. Callers: `code uses <name> --limit 20` for each changed signature. Add each caller's file to `paths`.
2. Reach: `code impact <name> --depth 2` for shared code.
3. Earlier work: `why <path>:<line>` and `precedents <id>`. Name the commit or `path#Symbol` to copy.
4. Library behavior: `mcp__docs__search_docs`. Name the doc page in the step.

## Write

Use `edit <id>` with `--set` for fields and `--section <name>` for sections.

| Part | Holds |
|---|---|
| `description` | 1-3 lines: what changes, for whom |
| `acceptance-criteria` | ``- [ ] `<command>` → <output that passes>``, narrowest crate first. A test filter states `N passed`. No clippy, fmt or full-check rows |
| `plan` | Numbered steps, one action each. Name `path#Symbol`, never line numbers. Per seam: what it catches, what it misses, the test choice (patch, add or none). Add `(new)` after the code span of a new file or symbol. Write an example path without the hash form |
| `components`, `paths` | Both required. `paths` lists every file allowed to change |
| tag | `size-s` (up to 15 developer requests) or `size-m` (up to 30). Bigger: propose a split, create nothing |

Name any outside type a step uses once, as `path#Type`.

## Verify

1. `task check <id>` prints `is ready`, `get <id>` shows every part, and `check` adds no problems.
2. Read the plan as the developer. Each step allows one result, and each name exists in the code or an earlier step.
3. Cut vague steps such as "add validation".
4. Give each likely failure a test or a check.
5. Reply in up to 3 lines: the plan, its size, what the owner decides.

## Stop

- Open product or scope question: run `questions`, then plan. Use an answer already in `## Notes`.
- Work already in history: name the commit and propose done or archived.
