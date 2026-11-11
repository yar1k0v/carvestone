---
name: close
description: Developer skill, last step of the toolbox. Ticks the passing criteria, writes the outcome, moves the task to review and runs the stone check.
argument-hint: "[task-id]"
---

# close (Developer)

1. Only after check reported every criterion with evidence.
2. One message, in this order:
   - `edit <id> --check-ac <n>` for each passing criterion;
   - `edit <id> --section outcome --text "<outcome>"`: 1–3 lines, what changed for the user and how it was checked; not a log;
   - `task move <id> review`;
   - `check`, then `get <id>#acceptance-criteria`.
3. No criterion may stay unticked; otherwise say which one is open.
4. Reply in at most 3 lines. No commit unless the owner asks.
