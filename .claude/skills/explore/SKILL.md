---
name: explore
description: First step of every toolbox. Loads the task (or question), profile and code context from stone in one batch; then find only what is still missing. /explore CS-n or /explore [question].
argument-hint: "[task-id or question]"
---

# explore

The stone command is `target/debug/stone.exe`.

## Steps

1. One message, all of these:
   - `get docs/crew-profile#profile`;
   - with a task: `get <id>`, `precedents <id> --budget 300`, `code explain <component> --budget 600` for each component, `git status --short`.
2. Raw task or a question, in this order, before any Grep or Read:
   - the app and module: `search "<key words>" --budget 300`, `arch hld`;
   - its design and components: `arch lld <container>`, `code explain <component> --budget 600`;
   - the code: `code find <Name> --limit 3` for each name it mentions, `code outline <file> --budget 300`, `code uses <name> --limit 20`.
3. Ready task: read the files in `paths`, and only the ranges the plan names. A file under 150 lines may be read whole. Nothing else is needed before editing.
4. Otherwise list what the next step still needs (a symbol's lines, a caller, a doc section, a library API) and get all of it in one message: `code find`, `code outline <file> --budget 300`, `code uses <name> --limit 20`, `code impact <name> --depth 2`, `code path <a> <b>`, `why <path>:<line> --budget 200`, `get <doc>#<anchor>`, the LSP tool for one symbol's definition or references, docs MCP `search_docs` for a library; Grep only for strings and config keys. Read only the ranges the next step changes or copies.
5. A question (no task): answer in a few lines with `path:line`, say what you checked and what you could not; change nothing.

A task is ready when it has a description, at least one acceptance criterion with a command, `components`, `paths`, and numbered `## Plan` steps.
