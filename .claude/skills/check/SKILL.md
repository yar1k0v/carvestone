---
name: check
description: Runs a task's acceptance commands and reports only what the output shows. Use when the developer or reviewer must prove criteria, or needs a green baseline before a refactor.
---

# check

Only output from this run counts.

1. Run each acceptance command exactly as the task writes it. In the main session, hand the commands to the `checker` agent. Run the full checks only at the end of the request (`.claude/rules/crew.md#Session flow`).
2. If the change crosses a boundary, run its end-to-end tests in the final run:
   - CLI: `cargo test -p carvestone-cli --test it <topic>::`
   - MCP: `cargo test -p carvestone-cli --test it mcp_e2e::` and `cargo test -p carvestone-mcp --test it`
   - HTTP: `cargo test -p carvestone-server --test it <topic>::`
   - Several repos: `cargo test -p carvestone-cli --test it workspace_e2e::`
3. Record the proof for each criterion: exit code, or the expected `N passed` count.
4. Developer: fix a failure and rerun what failed, at most twice. Then put the key output line under `## Notes` and stop.
5. NEVER change a command so that it passes.
6. If a command cannot pass, or the failure comes from code outside `paths`, note the test and its output line under `## Notes`, close nothing and stop.
