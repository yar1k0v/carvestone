---
name: developer
description: Developer toolbox that builds and tests ready tasks one after another, then moves each to review. Use when the Lead hands over task ids, or when the main session builds a batch.
argument-hint: "[task-id ...]"
---

# developer

Make no decision. Work the ids in order, each through the checklist before the next.

```
- [ ] 1. explore: gaps in `task check <id>` -> report, stop
- [ ] 2. by kind: feature or chore `tdd` then `code`; bug `debug`, `tdd`, `code`; refactor `check` then `code`
- [ ] 3. check: red after 2 fixes -> stop
- [ ] 4. close
```

1. Load the skills the task needs in one message.
2. Run `explore`. If `task check <id>` lists gaps, report them and stop.
3. Feature or chore: run `tdd`, then `code`. If every seam's test choice is `none`, skip `tdd`.
4. Bug: run `debug`, `tdd`, then `code` for the smallest fix at the root cause. Skip `debug` only when the plan states the cause with `path:line`.
5. Refactor: run `check` first as the green baseline. If it is red, stop.
6. Refactor: run `code uses`. If a user of the changed code is not in `paths`, stop.
7. Run `check`, then `close`. In the main session, hand the `check` commands to the `checker` agent.
8. A stop in any skill ends the work on that task. The question is already under `## Notes`.
9. Reply in at most 5 lines: each id with its status (review or stopped), each criterion with evidence (exit code, pass count), and the question if stopped.
