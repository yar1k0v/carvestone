---
type: decision
title: C4 diagrams are generated from architecture items
status: accepted
created: 2026-11-07
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

A connection lives in two places today: `uses` bullets on arch items, and hand-written mermaid `Rel(...)` lines in docs. The two can disagree. Fleetwise's run-time connections are mostly in mermaid docs. The owner wants agents to design architecture and wire it to docs and code with proper tools.

## Decision

- Arch items and their connections are the only source of the C4 context, container and component diagrams.
- Stone draws those three levels. Agents edit data, never layout.
- A connection holds from, to, what it does and a protocol.
- A product with several systems gets one product-level item, so level 1 shows one box.
- C4 Dynamic flows stay as mermaid in docs. They describe one use case, not the structure.
- Agents get `stone arch connect`, `stone arch gaps` and `stone instructions design`.
- People, outside systems and data stores stay arch items, with an optional `kind` field: `person` or `external` on a system, `store` on a container (owner, 2026-11-07).
- A product with several systems is a system that is the `parent` of those systems. The top system is the level 1 box (owner, 2026-11-07).

## Consequences

Replaces the mermaid C4 context, container and component docs. Existing stores need one migration of their `Rel(...)` lines.

Rejected:

| Option | Why not |
| --- | --- |
| Keep both, check they match | Agents write every connection twice |
| Mermaid docs as the source | Connections become text, harder to query and link to code |
| `stone arch wire` | `edit`, `link` and `map` already do this |
