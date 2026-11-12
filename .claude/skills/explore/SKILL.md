---
name: explore
description: Loads the task or question, the profile and the code context from stone in one batch, then finds only what is missing. Use when starting any task, plan or code question.
argument-hint: "[task-id or question]"
---

# explore

1. Send one message with `target/debug/stone get docs/crew-profile#profile`. With a task, add `brief <id>` and `git status --short`. Raise `--budget` only when the last line says `left out`.
2. For a raw task or a question, run these in order, before any Grep or Read:
   - the project: `map`;
   - the app and module: `search "<key words>" --budget 300`, `arch hld`;
   - its design and components: `arch lld <container>`, `code explain <component> --budget 600`;
   - the code: `code find <Name> --limit 3`, `code outline <file> --budget 300`, `code uses <name> --limit 20`.
3. For a ready task (`task check <id>` prints `is ready`), read the files in `paths`, and only the ranges the plan names. A file under 150 lines MAY be read whole.
4. Otherwise list what the next step still needs, and get all of it in one message:
   - `code find`, `code outline <file> --budget 300`, `code uses <name> --limit 20`;
   - `code impact <name> --depth 2`, `code path <a> <b>`, `why <path>:<line> --budget 200`;
   - `get <doc>#<anchor>`, the LSP tool for one symbol, docs MCP `search_docs` for a library;
   - Grep only for strings and config keys.
5. Read only the ranges the next step changes or copies.
6. For a question with no task, answer in a few lines with `path:line`. Say what you checked and what you could not. Change nothing.
