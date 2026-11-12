---
type: doc
title: Product strategy
status: draft
created: 2026-11-12T12:35:12+00:00
generated: { by: claude-code, at: '2026-11-12T12:35:18+00:00' }
description: 'What Stone works on next and why: five bets, the gap checklist with links, and the order of work.'
links: [ADR-21, docs/feature-agent-surface, M-18, M-19]
---
Decision proposed to the owner: Stone owns the whole task loop, and proves it outside this repo. Drawn from the day report of 2026-11-11. Status updated on 2026-11-12. Links: [[ADR-21]], [[docs/feature-agent-surface]], [[M-18]], [[M-19]].

## Position

- Goal stays: strong models shape docs, plans and tasks. Cheap models build ready tasks.
- Stone forces no agent. It gives clear instructions and honest answers.
- Held: one brief instead of three lookups cut worker context by about 30%, with the same answers.
- Held: greps per build fell from 14–30 to 12.
- Did not hold: a medium build still took 47 requests. 18 ran one check each, 11 gathered context.
- So the cost now sits in weak plans, the check loop and review returns, not in search.
- ⚠ Evidence limits: one repo, one model, 22 developer runs, one medium build after the changes. Every "likely" effect is untested.

## Bets

| # | Bet | Evidence | Effect | Test |
|---|---|---|---|---|
| 1 | The plan leaves nothing to look up: tests as `path#test`, a baseline count per test criterion, a test choice per seam, examples the gate cannot read as targets | 3 of 5 review returns came from plan gaps. 6 lookups in [[CS-240]] came from tests named without a file | likely | Build two medium tasks under the stricter check, then count requests and returns |
| 2 | A short worker loop: one command runs the acceptance commands and ticks what passes, one command closes a task | 18 of 47 requests ran one check. 41 requests in 22 runs were single store writes at close | likely | Decision needed: this makes Stone a task runner |
| 3 | Fix the faults seen in real output before more people rely on it | Each fault is in the checklist under "The brief and the map" and "Trust in answers" | certain | A wrong answer makes the agent stop trusting Stone and grep |
| 4 | Prove it outside this repo and with a cheap model | Every number so far comes from Carvestone building itself, with crew skills a new user lacks | certain gap | ADR-21 tests: another project, a new repo with only `stone instructions`, the same task built by a strong and a cheap model |
| 5 | Record tokens, turns and first-review result on each task | Three measurements needed a hand-written script each | likely | Open: can an agent report its own counts reliably? |

## Checklist

`[x]` means closed, and the item it links to holds the proof. `[ ]` means open.

### Planning and the ready gate

- [ ] A1. Plans name tests without a file path, so the brief gives no line range. 6 of 11 context requests in [[CS-240]].
- [ ] A2. Test criteria carry stale or missing baseline counts. Four tasks had a wrong `N passed`.
- [ ] A3. The gate reads example spans in a plan as real targets. Seen in [[CS-233]].
- [ ] A4. The gate does not ask a feature task for a doc link. Parked: the owner leaves it to the planner.
- [ ] A5. Plan gaps reach the reviewer, not the gate. [[CS-251]], [[CS-252]], [[CS-240]].
- [x] A6. CS-244 is planned and built: [[CS-244]].
- [ ] A7. The check-script follow-up is not planned. Open: which task holds it.

### The work loop

- [ ] B1. Each check is one request, and the final checks are four calls. 18 of 47 requests in [[CS-240]].
- [ ] B2. Web checks bypass `scripts/check.sh`. Four runs added 9k tokens.
- [ ] B3. Closing a task takes several store writes. 41 single-write requests in 22 runs.
- [ ] B4. Skills load in separate requests. 51 skill-only requests in 22 runs.
- [ ] B5. Most requests carry one tool call. 380 of 645.
- [ ] B6. A worker starts at about 23.8k tokens. About 4k is instruction files.
- [ ] B7. `stone edit --note` takes one note per call. Seen again in the [[CS-244]] review.

### The brief and the map

- [ ] C1. Entry points print `none found` for most components. Only the CLI has one.
- [ ] C2. The top precedent is noise: the 491-file release task [[CS-225]] matches every component.
- [ ] C3. The brief drops packages, key symbols and libraries of a component. Seen in the [[CS-250]] review.
- [ ] C4. A long task pushes the brief past its 2,000-token aim. [[CS-236]] brief: about 3,320 tokens.
- [ ] C5. A doc path in `paths` prints a `no outline` line. Seen in the [[CS-250]] review.
- [ ] C6. Nobody has tested that the session-start hook reaches the agent. [[CS-243]] is done. The live test in a new session is still open.
- [ ] C7. The hook command needs `stone` on PATH. The binary lives in the project folder on the owner's machines.
- [ ] C8. `stone map` takes 2.5–3.5 s, with about 1.3 s unexplained. Measured for [[CS-231]].
- [ ] C9. The map names `stone task next`, which is not on the agent surface.
- [ ] C10. The map lists a task under Ready although `task check` fails. Seen on [[CS-244]] on 2026-11-12.

### Trust in answers

- [ ] D1. `stone code find World::new` says no matches for an indexed symbol. Seen in the [[CS-252]] review.
- [ ] D2. Cut answers give only a count. Search, precedents and why are done in [[CS-244]]. The code lists are in [[CS-254]].
- [x] D3. Search hits past the SQL limit are uncounted. The line now says `and more not counted`: [[CS-244]].
- [ ] D4. An index line can go stale after a renumber or rename. `stone check` warns, `stone index` repairs.
- [ ] D5. No freshness proof: no item records the files and commit it was checked against. ADR-21 asks for it.
- [ ] D6. The index lock race is unexplained. Seen once, not reproduced in 25 runs. Likely real.
- [ ] D7. An agent cannot force a full index rebuild. The store guard blocks deleting the index.
- [ ] D8. The web refusal block depends on the text of an engine sentence. Seen in the [[CS-240]] review.
- [x] D9. Adding the session hook dropped CRLF endings in `.claude/settings.json`: [[CS-257]].
- [x] D10. `why` on a missing file said `run git add`: [[CS-258]].

### Proof and reach

- [ ] E1. No test on another project.
- [ ] E2. No test on a new repo that has only `stone instructions`.
- [ ] E3. No measured comparison of a cheap and a strong worker.
- [ ] E4. Tasks do not record tokens, turns or first-review result.
- [ ] E5. The evidence after the changes is one medium and one small build: [[CS-240]], [[CS-252]].
- [ ] E6. No release after v0.1.0. On 2026-11-12 `main` is 7 commits ahead of `origin/main`. The state of the public repo was not checked.
- [ ] E7. `scripts/check.sh` and one export branch are untested on Linux.
- [ ] E8. The session lead lost the goal after a context summary, and the owner re-sent it.

## Leave alone

- More read commands for agents. Only 27 of 194 greps were a search Stone could answer.
- A second versioning system. Git versions every item. Freshness proof (D5) is the smaller open part.
- An embedding model. No failure so far came from word search missing a match. Guess: revisit after a large doc set test.
- Forcing agents. Stone keeps to instructions and honest refusals.
- The 23.8k-token start. Most of it belongs to the agent host.

## Order of work

1. Close the day: run a privacy search, push, test the hook in a live session (C6), tag a release (E6).
2. Bet 3, small fixes first: C1, C2, C5, C9, C10, D1.
3. Bet 1: the stricter ready check (A1, A2, A3), then two medium builds as its test.
4. Bet 4: the outside-repo test and the cheap-model comparison. Its result decides how far to take bet 2.
5. Bets 2 and 5, once bet 4 shows they pay off outside this repo.

## Open questions

- Which task holds the check-script follow-up (A7)? Owner.
- Does bet 2 make Stone a task runner, and is that the product? Owner.
- Can an agent report its own request and token counts, or does the host have to? Lead, before bet 5.
