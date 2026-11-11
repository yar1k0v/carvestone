---
name: check
description: Developer and Reviewer skill. Runs the task's acceptance commands and the full checks, and reports only what the output shows. Also gives a green baseline before a refactor.
argument-hint: "[task-id]"
---

# check (Developer, Reviewer)

Only output from this run counts. An earlier run, or a belief that it should pass, doesn't.

1. Run each acceptance command exactly as the task writes it, then the full checks in the crew rules for the touched areas.
2. If the change crosses a boundary, run its end-to-end tests too:
   - CLI: `cargo test -p carvestone-cli --test it <topic>::`
   - MCP: `cargo test -p carvestone-cli --test it mcp_e2e::` and `cargo test -p carvestone-mcp --test it`
   - HTTP: `cargo test -p carvestone-server --test it <topic>::`
   - Several repos: `cargo test -p carvestone-cli --test it workspace_e2e::`
3. For each criterion, record the proof: exit code, or the `N passed` count you expected.
4. Developer: fix a failure and rerun what failed, at most twice. Then put the key output line under `## Notes` and stop.

Never change a command so that it passes. If a command can't pass, or the failure comes from code outside `paths`, note the test and its output line under `## Notes`, close nothing, and stop.
