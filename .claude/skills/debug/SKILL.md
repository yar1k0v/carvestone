---
name: debug
description: Developer skill for a bug. Finds the real cause with a failing command before anyone writes a fix. Runs after explore.
argument-hint: "[task-id]"
---

# debug (Developer)

Know the cause before you change the code.

1. Find one quick command that shows this bug: a failing test, or a `stone` run on a small fixture store.
2. Compare its output with the task's report. A different symptom means a different bug, so keep looking.
3. In the same request, ask stone what changed: `why <path>:<line>`, `history <component>`, `code impact <name> --depth 2`.
4. List up to 4 possible causes, likeliest first, each with the result that would confirm it. Test them one by one, changing one thing per test.
5. Name the cause in one line with `path:line`. In `tdd`, the failing command then becomes a case, patched into the closest test first.

Mark temporary logging with `DEBUG-<task id>` and remove all of it before close. Redact secrets in anything you quote.

Stop, with what you learned under `## Notes`, when:

- 3 causes are ruled out;
- the fix reaches outside `paths`;
- the fix needs a decision;
- the plan's stated cause is wrong.
