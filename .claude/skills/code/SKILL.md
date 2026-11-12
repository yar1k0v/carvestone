---
name: code
description: Makes the planned edits layer by layer with quick checks, inside the task's paths and plan. Use when a ready task is explored and needs its code or docs changed.
argument-hint: "[task-id]"
---

# code

1. Run `task move <id> in-progress` in the same message as the first Reads, unless the task is already in progress.
2. Fill gaps in the same request as the Reads:
   - a definition or fields: `code outline <qualified name>`, or LSP (workspaceSymbol, then goToDefinition or hover). Never Grep a generated file;
   - callers: `code uses <name>`, or LSP findReferences;
   - an API this repo does not already use the same way: `mcp__docs__search_docs`;
   - text the graph does not hold: `git grep -n "<text>" -- <dir>`. Never `grep -r`.
3. Edit one layer per batch, in the profile's layer order: every Edit of the layer in one message, across files. Then run the layer's quick check: LSP diagnostics (`.claude/rules/crew.md#Working`).
4. When every step is done and the quick checks are green, go to `check`.
5. Refactor: keep behavior and leave the build green after each step. Put a fix you notice under `## Notes`.
6. Feature: copy the pattern the plan names. UI is done only when the screen matches its board on the design canvas (ADR-14), in light, dark and phone views.

Stop, with the reason under `## Notes` (`edit <id> --section notes`), when:

- there is no ready task;
- the code contradicts the task or plan, or a decision is needed;
- a file outside `paths` must change;
- the work passes the size budget (size-s 15, size-m 30 requests). Note where it stands.
