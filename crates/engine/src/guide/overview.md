# What stone holds

Stone keeps this repository's tasks, docs, decisions and architecture as markdown in `stone/`, and answers code questions from an index. Read and change all of it with the `stone` CLI, never by editing `stone/` files. Answers are only as long as the question, so ask narrowly.

Ids: tasks use the project's prefix (`stone list --type task` shows it), milestones `M-n`, decisions `ADR-n`; docs, plans, references and architecture use paths (`docs/<slug>`, `plans/<slug>`, `refs/<slug>`, `arch/<path>`). `<id>#<section>` points at one heading.

1. Find: `stone search "words" --budget 200`, then `stone get <id>` or `stone get <id>#<section>`. `stone list --type task --status todo` lists items.
2. Take work: `stone task next` lists ready tasks; `stone task move <id> in-progress` takes one.
3. Understand the code: `stone arch hld` for the parts, `stone code explain <component>` for one part, then `stone code find <Name>`, `stone code outline <file>`, `stone code uses <name>` and `stone code impact <name>`. Read only the lines you need after that. Answers marked inferred, or a `note:` line, mean: run `stone code native` once.
4. Learn from the past: `stone why <path>[:line]` and `stone precedents <task>`.
5. Record as you go: `stone edit <id> --section <name> --text "..."`, `--set key=value`, `--check-ac <n>`, `--note "<text>"`. `stone new <type> --title "..."` creates an item and `stone link <from> <to>` connects two. A fact belongs to the component or decision it is about, not to the task.
6. Finish: write the outcome section, run `stone task move <id> done`, then `stone check` until it prints `no problems`.

Fix-ups: `stone task remove`, `stone rename`, `stone delete` and `stone renumber` refuse a change that would break links, and say why.
Writes are safe to repeat: a second run prints `no change`. Agents other than Claude Code set `STONE_AGENT=<name>`.
Library docs are not in stone: use the Docs MCP tool `search_docs` with the version in a component's `libraries`.
More: `stone instructions plan`, `stone instructions do`, `stone instructions finish`, `stone instructions design`, `stone instructions debug`. A project can replace any topic with a section of `docs/instructions` headed by the topic's name.
