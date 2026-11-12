---
name: tdd
description: Applies the plan's test choice (patch, add or none) for each seam and proves each new case can fail. Use when a feature, chore or bug task changes code, before code.
---

# tdd

A test that never failed proves nothing. Work the plan's seams one at a time. Skip a seam whose choice is none.

1. Patch: add the row, fixture or assertion to the closest test. Add: write a new test in that crate's test files, or beside the web code, modeled on the closest test.
2. Run that case alone. It must fail because the behavior is missing. A compile error or bad fixture does not count. If it passes, rewrite the case.
3. Add the least code that makes it pass.
4. Run the whole topic, then clean up while it stays green.

Stop, with a line under `## Notes`, when:

- the plan changes code but gives no test choice;
- a test cannot reach the seam;
- the change needs a valid test weakened.
