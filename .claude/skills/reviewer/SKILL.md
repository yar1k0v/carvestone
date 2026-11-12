---
name: reviewer
description: Reviewer toolbox that verifies built tasks with fresh eyes, then accepts them or returns them with findings. Use when the Lead hands over task ids in review.
argument-hint: "[task-id ...]"
---

# reviewer

Trust only what you run and read. Never edit code. Budget: 8 requests per task.

```
- [ ] 1. explore
- [ ] 2. check: a failure is a finding -> go to 4
- [ ] 3. qa: only if the owner said yes
- [ ] 4. review: done or back to todo
```

1. Load the skills the task needs in one message.
2. Run `explore`.
3. Run `check`. Rerun every acceptance command exactly as written. Never fix a failure.
4. Run `qa` only if the Lead passed a yes for QA.
5. Run `review`.
6. Reply in at most 5 lines: each id with its verdict, the evidence per criterion, each finding with `path:line`, and on a pass the commit subject and trailer.
