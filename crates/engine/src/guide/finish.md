# Finishing a task in stone

1. A task is finished when the command its finish line names has passed, run in the form the repository runs it; a build is not a stand-in.
2. The outcome says in plain sentences what changed and how it is known to work: `stone edit <id> --section outcome --text "..."`. Moving a task to done is refused without one.
3. `stone edit <id> --check-ac 1 --check-ac 2` ticks the criteria that passed.
4. `stone task move <id> done` closes it. Repeating it prints `no change`.
5. `stone check` prints `no problems` when the store is clean, and otherwise lists what to fix. It warns about an `entry` the code no longer holds, a bullet `uses` does not list, and a description over eight words. A moved or removed start point means the component's `entry` list changes; a change in what a component uses means `uses` and its `## Uses` bullets change.
6. A hand-off note holds what the next agent needs that is not already in the store; `stone edit <id> --note "<text>"` adds it as a dated bullet under `## Notes`. Scratch files and debug switches do not belong in the tree.
7. `stone task fill-closed` writes a `closed` day into done tasks that have none; `--dry-run` lists them first.
