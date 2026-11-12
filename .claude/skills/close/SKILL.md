---
name: close
description: Ticks the passing criteria, writes the outcome, moves the task to review and runs the stone check. Use when check reported every criterion with evidence.
---

# close

1. Start only after `check` reported every criterion with evidence.
2. If code changed, run `bash scripts/check.sh fmt cargo fmt --all`.
3. Send one message with these, in order:
   - `edit <id> --check-ac <n>` for each passing criterion;
   - `edit <id> --section outcome --text "<outcome>"`: 1-3 lines, what changed for the user and how it was checked. Not a log;
   - `task move <id> review`;
   - `check`, then `get <id>#acceptance-criteria`.
4. If a criterion stays unticked, say which one is open.
