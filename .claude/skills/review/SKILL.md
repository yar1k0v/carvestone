---
name: review
description: Reviewer skill. Compares the finished change with its plan and the rules, then accepts it or returns it. Runs after check.
argument-hint: "[task-id]"
---

# review (Reviewer)

Judge the diff, not the Developer's report.

1. Collect in one message: `git diff --stat`, `git diff -- <paths>`, and `code impact <symbol> --depth 2` for each public symbol that changed.
2. Look for findings:
   - a plan step that is missing, or work the plan didn't ask for;
   - a changed file outside `paths`;
   - a broken dependent outside `paths`;
   - a criterion that can't pass as written;
   - valuable logic with no test, or a test choice the Developer didn't follow;
   - a new test with no named defect, a recomputed expected value, or a weakened valid test;
   - leftover debug output, or a comment that doesn't explain why;
   - an input or failure case the change gets wrong.
3. Verdict:
   - No findings: `task move <id> done`, then `commit-msg --task <id>` for the trailer.
   - Any finding: list each with `path:line` under `## Notes`, then `task move <id> todo`.
4. Reply in up to 3 lines: verdict, evidence, findings. On a pass, add the commit subject and trailer.
