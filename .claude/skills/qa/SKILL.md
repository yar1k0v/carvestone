---
name: qa
description: Uses the changed feature the way a real user does, through the CLI, MCP tools or web UI, and reports what happened. Use when the owner said yes to a QA pass, after check.
argument-hint: "[task-id ...]"
---

# qa

Trust only what you see.

1. From the tasks, list the user flows to try. Add one wrong-input case and one empty-store case.
2. Run `cargo build --bin stone`. Compare the new output with `docs/agent-scratch/qa-before/`. If nothing is saved, report the new output only.
3. For a CLI or MCP change, run each flow with `target/debug/stone.exe`. Check that:
   - the output answers what the user asked;
   - `--json` carries the same facts;
   - an error is one plain sentence that names the next step.
4. For a screen change, drive the web UI with `playwright-cli`.
   - Act from `snapshot` refs. Take screenshots only for the design comparison.
   - Count any console error as a failure.
   - Compare with the task's board on the design canvas (ADR-14) at 1440x900, in the dark theme, and at 390x844.
   - If the canvas is out of reach, list the screenshot paths under `## Notes` and claim no match.
5. Report each flow as passed or failed, with its evidence: the output line, console count or screenshot path.

⚠ Run flows that write on a copy of a store in `docs/agent-scratch/`, never on this repo's store or a real project's store.

A failure is a finding for `review`: do not fix it. If the web UI is not running, write that under `## Notes` and stop. Starting it is the owner's call.
