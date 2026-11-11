---
type: decision
title: First public release keeps four linked parts and cuts migration and inferred patterns
status: proposed
created: 2026-11-07
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

Carvestone goes public on GitHub. The owner wants a sharp tool with a clear definition. The PRD lists five parts and replaces four tools. Separate tools cannot link a task to a symbol or a commit to a decision.

## Decision

Carvestone is: "Your project's plan, docs, architecture and code graph, linked in one git repo, for people and AI agents."

- Keep: planner, docs site, architecture map, code graph.
- Keep git links (`why`, `history`, `timeline`, commit trailers). They join tasks and decisions to code.
- Keep `precedents`.
- Cut: `stone import` and the importers.
- Cut: CS-82, two-way Jira and ClickUp sync.
- Cut: M-13, inferred patterns, topics and drift per service.

## Consequences

The README leads with the sentence above. The claim is the links, not the feature count.

Rejected:

| Option | Why not |
| --- | --- |
| Agent memory leads, UI secondary | The owner keeps all four parts as equals |
| Architecture map leads | The owner keeps all four parts as equals |
| "One binary replaces four tools" | Invites a feature comparison with each tool |
