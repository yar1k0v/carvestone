---
name: close
description: Ticks the passing criteria, writes the outcome, moves the task to review and runs the stone check. Use when check reported every criterion with evidence.
---

# close

1. Start only after `check` reported every criterion with evidence.
2. If code changed, run `bash scripts/check.sh fmt cargo fmt --all`.
3. Send one message with these, in order:
   - one call that ticks, writes and moves: `stone edit <id> --check-ac <n> --check-ac <m> --section outcome --text "<outcome>" --set status=review`. The outcome is 1-3 lines, what changed for the user and how it was checked. Not a log. `edit --set status` runs the same status checks as the move command;
   - `check`, then `get <id>#acceptance-criteria`.
4. Add notes with `--note`, which repeats, in the same call.
5. If a criterion stays unticked, say which one is open.
