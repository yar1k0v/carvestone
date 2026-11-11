# Doing the work in stone

1. `stone task move <id> in-progress` marks the task taken, so the store shows who has it.
2. `stone brief <id>` prints the task with its linked doc sections, its components with entry points, and finished tasks like it. For the rest, `stone search "words"` finds an item and `stone get <id>#<anchor>` prints one section.
3. Edits fit the store as the work goes: `stone edit <id> --section <name> --text "..."`. Fields take `--set key=value`; lists take `a,b`, `+a`, `-a`.
4. `stone edit <id> --check-ac <n>` ticks an acceptance criterion as it passes. `stone edit <id> --note "<text>"` adds a dated bullet to `## Notes` without rewriting the section; the same text twice adds it once.
5. A new fact has an owner. The component or decision holds it, edited with `stone edit` or created with `stone new` and pointed at with `stone link`; the task does not.
6. Library docs are not in stone. A component's `libraries` list gives the library and version for Docs MCP `search_docs`.
7. When reality disagrees with the plan, the plan changes in the store the same day; `stone check` shows what broke.
8. `stone commit-msg` prints the `Refs:` trailer lines for the task in progress, which puts the task in the commit.
9. A task made by mistake or only to try something is deleted with `stone task remove <id>` (`--dry-run` shows the verdict first); stone refuses when another item links to it or a commit names it, and says which. A task that was real work and is no longer wanted stays in the store as `stone task move <id> archived`. Deleting task files by hand skips these checks and leaves broken links. `stone rename <id> <new-id>` gives a doc, plan, reference or architecture item a new id and rewrites its links; `stone delete <id>` is refused while items link to it. After a merge two files can share a number: `stone renumber <path>`.
