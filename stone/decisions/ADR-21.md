---
type: decision
title: Stone is a linked context store, built for agents first
status: proposed
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
links: [ADR-16]
---

## Context

Agents on a Stone project still take many turns and grep files. Three causes showed in one working session on 2026-11-11:

- The instructions list more than ten read commands in a fixed order. Each step costs a turn, and every turn sends the whole context again.
- A wrong or empty answer ends trust. CS-223 says "nothing uses" for a constant the code uses.
- The agent sets `--budget` before it knows the size of the answer. The numbers in use were copied from the skill text.

A task with an exact plan cost about 35k worker tokens and passed its first review (CS-226, CS-227). A task with one wrong statement in the plan cost 76k tokens and three rounds (CS-224).

The owner's `project_map` tool for the Pi agent prints a workspace map at session start. The owner reports a large gain from it.

## Decision

The owner confirmed each line on 2026-11-11.

- **Claim:** Stone is one linked store of docs, plans, tasks, architecture and code. An LLM gets the context of its task from it in one or two calls. ADR-16 stays: the claim is the links.
- **Quality bar:** each part is the best for agents. The measure is what an LLM gets from it: correct answers for few tokens, tasks a worker finishes without questions, architecture read in one call. Features for people stay good enough.
- **Read surface:** two pushes and two pulls. Stone pushes a project map at session start and a task brief at task start. The agent pulls with `search` and `get`. The other read commands stay in the binary and leave the agent instructions.
- **Output size:** Stone decides it, in tiers. The critical tier is never cut: the task's criteria, its plan and its direct links. Only the optional tier shrinks. Every answer states what it left out, and how large that is. `--budget` stays as an override.
- **Honest answers:** when Stone does not index something, the answer says so. It never says "nothing uses".
- **Versioning:** proof of freshness. An item or section records the files and the commit it was checked against. Stone marks it stale when those files change. Git keeps the history of items, as today.
- **Workers:** one call returns the task pack: criteria, plan, linked doc and design sections, and the named code with outlines. Search is the fallback.
- **Ready check:** before a task counts as ready, Stone checks it: the paths exist, the symbols resolve, and each criterion carries a command.
- **Roles:** strong models shape features, plans, milestones and tasks. Cheap models build ready tasks. Stone forces no agent.
- **Measure:** each task records tokens, turns, and pass or return at first review, by size tag and model.
- **Order of work:** trust first, then the map, then the brief.
- **Dogfood:** Carvestone is built with Stone. Two more tests cover what this repo cannot: the owner's other projects, and a new repo that has only `stone instructions`.

## Consequences

- The agent guide (`stone instructions`) shrinks to the two pushes, the two pulls and the write commands.
- `stone brief` (CS-202) becomes the task brief and the task pack.
- The session-start map is new. Its data already feeds the Now page of the web UI.
- Staleness moves from one check per component to one per item or section.
- ⚠ Not measured yet: no count of turns or tokens per task exists from before this decision. Take it from the transcripts before the first change, or no later gain can be shown.
- ⚠ Unknown: whether a cheap model stops when a plan step cannot give its stated result. Test one ready task on a strong and a cheap model.

Rejected:

| Option | Why not |
| --- | --- |
| Match the wiki, graph, architecture and backlog tools feature for feature | Four roadmaps for one maintainer, in a race on each tool's home ground |
| Keep all read commands and write a better guide | The agent still chooses among ten tools, and habit favours grep |
| One command for every question | Stone must guess intent from free text, where its hints are weakest |
| The agent sets the budget | The number is a guess, and a low guess cuts what matters |
| No output cap | Later turns send every large answer again |
| A version number on each document | Upkeep on every edit, and no reader needs the number |
| A built-in doc generator | Agents already write docs through the CLI |
