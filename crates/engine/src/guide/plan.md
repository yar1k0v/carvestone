# Planning in stone

Plans and tasks live in the store, so the next agent finds them without the chat.

1. Existing items: `stone get <plan-id>` and `stone search "the words of the task"` show what is already written, so an item that already says it is reused.
2. A task is one step that can finish on its own. `stone new task --title "..."` takes the next number and accepts `--set milestone=M-1 --set priority=high --set depends_on=<id>,<id>`; numbers are never typed by hand.
3. The description holds the purpose: `stone new task --section description --text "..."` or `stone edit <id> --section description --text "..."`.
4. Acceptance criteria are the finish line, a checkbox per command to run: `--section acceptance-criteria --text "- [ ] <the command that proves it>"`. A task with no finish line is only a note.
5. A task's plan section can name the files it may touch, which tells the next agent where to work.
6. `depends_on` orders the work, and `stone task next` lists the tasks that are ready.
7. `stone link <plan-id> <task-id>...` ties a plan to its tasks, so `stone get` shows them and `stone search --from <id>` ranks them first.
8. A component records where it starts and what it does with the others. `--set entry=src/main.rs#main` lists its entry points as `path#Symbol` or `path`, and `stone code entries <id>` shows what the code graph detects. `uses` lists the components it depends on, and a `## Uses` section says what it does with each: one bullet per target, `- [[arch/store]]: Saves items`, a verb phrase of two to five words, at most eight. The bullet gives the purpose, not the mechanism. When the link leaves the process, the bullet ends with how it works in parentheses, `- [[arch/server]]: Reads and edits items (HTTP)` (or gRPC, queue, SQL, file); no code edge is expected for it, so `stone check` does not warn that `uses` lacks one. A bullet without it, or with an in-process one, still warns.
9. How the parts connect at run time is data: `stone arch connect <from> <to> --does <verb phrase> [--over <protocol>]` writes one connection, and `stone arch c4` draws the diagrams from it; `stone instructions design` has the order. Only a flow through several parts is a `C4Dynamic` mermaid fence in a doc.

A plan names the milestone it serves with `--set milestone=M-n`, and its tasks carry the same milestone.
