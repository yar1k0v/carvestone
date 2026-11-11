---
type: decision
title: The architecture UI is two C4 diagrams, system architecture and solution architecture
status: proposed
created: 2026-11-07
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

The owner cannot read the architecture UI. It mixes a system map, a domain view, a low-level view and service pages. A stranger must learn what the parts are and how they talk within 60 seconds. The owner named the C4 model (https://c4model.com/) as the form.

## Decision

The System space shows two diagrams:

| Diagram | C4 level | Shows |
| --- | --- | --- |
| System architecture | 1, System context (landscape when a store has several systems) | The product, its users and outside systems |
| Solution architecture | 2, Container | The apps and data stores inside one system, and how they talk |

- Clicking a container opens its C4 level 3 Component diagram.
- Level 4 is not drawn. A component links to the code graph.
- Every diagram has a title with its type and scope, and a legend.
- Every box states its type and a short description. Containers and components also state a technology.
- Every line has one direction and a specific label. Container lines state the protocol.
- The domain view, low-level view and system map pages go. Anything not on these diagrams or a component's page goes.

## Consequences

Replaces the Architecture page part of ADR-13 and refines the System space of ADR-14. The diagram design goes on the design canvas before anyone builds it (ADR-14).

The owner confirmed the mapping on 2026-11-07: system architecture is level 1, solution architecture is level 2.
