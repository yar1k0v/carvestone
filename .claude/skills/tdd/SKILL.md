---
name: tdd
description: Developer skill. Applies the plan's test choice for each seam (patch, add or none), and proves each new case can fail.
argument-hint: "[task-id]"
---

# tdd (Developer)

A test that never failed proves nothing. Work through the plan's seams one at a time, and skip a seam whose choice is none.

For each other seam:

1. Patch: add the row, fixture or assertion to the closest test. Add: write a new test in that crate's test files, or beside the web code, modeled on the closest test.
2. Run that case alone. It must fail because the behavior is missing. A compile error or bad fixture doesn't count, and a pass means the case needs rewriting.
3. Add the least code that makes it pass.
4. Run the whole topic, then clean up while it stays green.

Stop, with a line under `## Notes`, when:

- the plan changes code but gives no test choice;
- a test can't reach the seam;
- the change needs a valid test weakened.
