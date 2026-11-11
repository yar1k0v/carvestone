---
type: decision
title: HLD and LLD are views of the architecture, not new item types
status: accepted
created: 2026-10-31
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

Stone draws C4 diagrams (context, container, component, dynamic), keeps architecture items (system, container, component) and, with M-12, opens a service from the diagram into modules, entry points and classes. Nothing names the two levels or lets a reader ask for one. The Fleetwise project already writes about 86 low-level pages (flows per service) by hand with agents, so the need is real. A high-level design (HLD) says what the product is made of and how the parts talk. A low-level design (LLD) says how one part is built inside.

## Decision

Levels come from what already exists; no new item types.
- HLD: the system and container items, C4 Context and Container diagrams, how containers talk at run time (HTTP, queues, databases, outside systems), and the topics each container reads and writes. Hand-drawn for what cannot be detected; topics and code-backed links are computed.
- LLD: one container opened up. Its components and modules, entry points, patterns (API, messaging, data access, structure), key flows as C4 Dynamic diagrams or docs, and classes down to the code page. Computed from the index and never written to items; flows are written by hand or by agents.
- Level is derived, not typed: system and container items and C4 Context and Container diagrams are HLD; component items and C4 Component and Dynamic diagrams are LLD. A doc may say level: lld in its frontmatter and link the container it describes; a flow doc names its start point as path#Symbol.
- Shown in the web UI: the Architecture page has an HLD view (context, containers, topics) and an LLD view per container (the service page of M-12: diagram, modules, entry points, patterns, flows), reached from each other by clicking a container or a Back to HLD link.
- Shown in the CLI: stone arch hld prints the HLD for a product, stone arch lld <container> prints the LLD, both from the same engine answer as the UI.
- Kept true: stone check warns when a flow doc or diagram names a file or symbol the code no longer has, and reports drift both ways (CS-97).

## Consequences

Builds on M-12 and M-13: the LLD view is the service page, the patterns and topics are its content. Cost is small for the names, the level field and the CLI views, larger for the UI view and the flow check. Rejected: separate hld and lld item types, because a second copy of the architecture would drift from the first, and one page for both, because an HLD reader wants five boxes and an LLD reader wants five hundred.
