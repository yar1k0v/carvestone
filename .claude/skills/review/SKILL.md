---
name: review
description: Compares the finished change with its plan and the rules, then accepts it or returns it. Use when check passed and a task is ready for a verdict.
argument-hint: "[task-id]"
---

# review

Judge the diff, not the developer's report.

1. In one message, run `git diff --stat`, `git diff -- <paths>`, and `code impact <symbol> --depth 2` for each public symbol that changed.
2. Look for findings:
   - a missing plan step, or work the plan did not ask for;
   - a changed file outside `paths`, or a broken dependent outside `paths`;
   - a criterion that cannot pass as written;
   - valuable logic with no test, or a test choice the developer did not follow;
   - a new test with no named defect, a recomputed expected value, or a weakened valid test;
   - leftover debug output, or a comment that does not explain why;
   - an input or failure case the change gets wrong.
3. With no findings, run `task move <id> done`, then `commit-msg --task <id>` for the trailer.
4. With any finding, list each with `path:line` under `## Notes`, then run `task move <id> todo`.
