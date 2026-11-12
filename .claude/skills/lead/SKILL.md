---
name: lead
description: Lead toolbox that works with the owner, settles design, plans ready tasks and hands batches to the developer and reviewer. Use when the owner starts a session, brings a topic, or names a task id.
argument-hint: "[topic or task-id]"
---

# lead

Run the session by `.claude/rules/crew.md#Session flow`. Report as to a product owner: what a user can do now, what comes next, what needs a decision.

```
- [ ] 1. goal and explore (no task yet: start from `map`)
- [ ] 2. define: `questions` until the owner confirms
- [ ] 3. task: `target/debug/stone.exe new task --title "..." --set milestone=M-n --set components=<arch ids>`
- [ ] 4. plan: `planning`. Go on only when each `task check <id>` prints `is ready`
- [ ] 5. qa-before: save the old output of each changed CLI flow
- [ ] 6. build
- [ ] 7. review
- [ ] 8. full checks
- [ ] 9. commit and push, after the owner approves
```

## Run mode

Batch is the default. Switch only when the owner asks:

- Approve each go: the owner answers each decision as it comes up.
- Auto-approve loop: `/loop` takes the ready tasks to done and follows every recommended option. Record each approval under the task's `## Notes`. Stop on a real bug outside the task, a gate the plan says must hold, or the same check failing twice.

## Tiny docs fix (trial)

Applies to a task in review, or a fix after the reviewer returned it, when the change is docs only (Markdown or a store item's text, no code, config, test or workflow file) and `git diff --numstat -- <paths>` totals 10 lines or fewer.

1. Read the diff and run the acceptance commands in `checker`.
2. Run `target/debug/stone.exe task move <id> done` and add `Lead accepted: tiny docs fix, <n> lines` under `## Notes`.
3. Ask no QA question and start no reviewer round.
4. After 3 tasks carry that Notes line, report their token use with `AskUserQuestion`: keep or drop. On drop, remove this section.
