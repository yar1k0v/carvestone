# What stone holds

Stone keeps this repository's tasks, docs, decisions and architecture as markdown in `stone/`, and answers code questions from an index. Read and change all of it with the `stone` CLI, never by editing `stone/` files.

Ids: tasks use the project's prefix, milestones `M-n`, decisions `ADR-n`; docs, plans, references and architecture use paths (`docs/<slug>`, `plans/<slug>`, `refs/<slug>`, `arch/<path>`). `<id>#<section>` points at one heading.

1. Start a session: `stone map` prints the components with their code and entry points, the tasks in review, the next ready tasks, milestone progress and git state.
2. Start a task: `stone task move <id> in-progress` takes it, and `stone brief <id>` prints the task, its linked doc sections, its components with entry points, and finished tasks like it.
3. Anything else: `stone search "words"`, then `stone get <id>` or `stone get <id>#<section>`.
4. Record as you go: `stone edit <id> --section <name> --text "..."`, `--set key=value`, `--check-ac <n>`, `--note "<text>"`. `stone new <type> --title "..."` creates an item and `stone link <from> <to>` connects two. A fact belongs to the component or decision it is about, not to the task.
5. Finish: write the outcome section, run `stone task move <id> done`, then `stone check` until it prints `no problems`.

Fix-ups: `stone task remove`, `stone rename`, `stone delete` and `stone renumber` refuse a change that would break links, and say why.
Writes are safe to repeat: a second run prints `no change`. Agents other than Claude Code set `STONE_AGENT=<name>`.
Library docs are not in stone: use the Docs MCP tool `search_docs` with the version in a component's `libraries`.
More: `stone instructions plan`, `stone instructions do`, `stone instructions finish`, `stone instructions design`, `stone instructions debug`. A project can replace any topic with a section of `docs/instructions` headed by the topic's name.
