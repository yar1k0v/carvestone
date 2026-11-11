---
name: code
description: Developer skill. Makes the planned edits layer by layer with quick checks, staying inside the task's paths and plan; never decides.
argument-hint: "[task-id]"
---

# code (Developer)

You execute the plan and make no decisions.

## Steps

1. `task move <id> in-progress` in the same message as the first Reads (if not already in progress).
2. Fill gaps in the same request as the Reads, in this order:
   - a type's fields or a symbol's definition explore didn't show: `code outline <qualified name>`, or `LSP` (workspaceSymbol by name, then goToDefinition or hover at its line); never Grep a generated file;
   - callers: `code uses <name>`, or `LSP` findReferences;
   - a library or framework API this repo doesn't already use the same way: `mcp__docs__search_docs` (library and version from the component's `libraries`);
   - `LSP` and `mcp__docs__*` are deferred tools: load the ones you need with `ToolSearch select:LSP,mcp__docs__search_docs` in the same message as the first Reads;
   - text the graph doesn't hold: `git grep -n "<text>" -- <dir>`, never `grep -r` (it also hits `target/` and `node_modules/`).
3. Edits, one layer per batch, in the profile's layer order: every Edit of the layer in one message, across files. Run that layer's quick check from the checks in the next batch (a subagent gets no LSP diagnostics with its edit results; `cargo check -p <crate>` or `pnpm --dir web typecheck` is the type check).
4. Every step of the plan done and the quick checks green: hand over to check.

By task kind:
- refactor: no behavior change; every step leaves the build green; a fix you notice goes under `## Notes`, not into the change.
- feature: copy the pattern the plan names. UI is done only when the screen matches its board on the design canvas (ADR-14), in light, dark and phone views.

## Stops

- No ready task: say so and stop. The Lead plans it first.
- The code contradicts the task or plan, or a decision is needed: under `## Notes` (`edit <id> --section notes`), stop.
- A file outside `paths` must change: under `## Notes`, stop.
- Over the size budget (size-s 15, size-m 30 requests): where the work stands under `## Notes`, stop.
