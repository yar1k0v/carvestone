---
name: qa
description: Reviewer skill. Uses the changed feature the way a real user does, through the CLI, MCP tools or web UI, and reports what happened. Optional: runs after check, only when the owner asks for a QA pass.
argument-hint: "[task-id]"
---

# qa (Reviewer)

Tests prove the code. QA proves the product. Use the feature as a user would, and trust only what you see.

1. From the task, list the user flows to try. Add one wrong-input case and one empty-store case.
2. Build first: `cargo build --bin stone`.
3. CLI or MCP change: run each flow with `target/debug/stone.exe`. Check that:
   - the output answers what the user asked;
   - `--json` carries the same facts;
   - an error is one plain sentence that names the next step.
4. Screen change: drive the web UI with `playwright-cli` (read only the skill section you need).
   - Act from `snapshot` refs, and take screenshots only for the design comparison.
   - Any console error is a failure.
   - Compare with the task's board on the design canvas (ADR-14), at 1440×900, in the dark theme, and at 390×844.
   - No access to the canvas: list the screenshot paths under `## Notes` for the owner, and don't claim a match.
5. Report each flow as passed or failed, with its evidence: the output line, console count or screenshot path.

⚠ Flows that write run on a copy of a store in `docs/agent-scratch/`, never on this repo's store or a real project's store.

A failure is a finding for `review`, so don't fix it. If the web UI isn't running, write that under `## Notes` and stop: starting it is the owner's call.
