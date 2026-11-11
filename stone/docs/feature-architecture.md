---
type: doc
title: Architecture
status: stable
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: Systems, containers and components that own code, drawn as C4 diagrams.
components: [arch/carvestone/stone/engine, arch/carvestone/stone/graph, arch/carvestone/stone/cli, arch/carvestone/web-ui/web]
links: [ADR-8, ADR-13, ADR-17, ADR-18, ADR-20]
---

Stone holds the design as one file per system, container and component in `stone/arch/`, and draws C4 levels 1 to 3 from them.

Status: live

## What it does

- Each element is a file in `stone/arch/`, nested by folder ([[ADR-8]]).
- Fields: `code` globs, `entry`, `technology`, `uses`, `libraries`, `status`. Sections: Responsibilities, Uses, Interfaces, Gotchas.
- Ownership: each tracked code file under `code_roots` belongs to exactly one element, and the element below wins when two globs match. `stone check` reports a code file that no element owns, or that two elements own when neither is below the other. Files that Stone does not read as code (art, config, docs) need no owner.
- People and agents write each connection: from, to, what it does, and a protocol. Agents write data, never layout ([[ADR-20]]).
- The web UI shows two diagrams. System architecture is C4 level 1. Solution architecture is level 2. A click on a container opens its level 3 diagram ([[ADR-18]]).
- `stone arch hld` prints the high-level design. `stone arch lld <container>` prints one container ([[ADR-13]]).
- Stone checks only the links the build proves: imports, shared libraries and project references. It never calls a written run-time connection wrong ([[ADR-17]]).
- The code graph reports drift: the code depends on a component that `uses` does not list, or the reverse.
- Tests, benchmarks and generated code count for no element's dependencies.

## Commands

| Command | Purpose |
| --- | --- |
| `stone arch hld` | Print the design of the whole product |
| `stone arch lld <container>` | Print the design of one container |
| `stone arch c4` | Print a diagram as Mermaid text |
| `stone arch connect` | Write one connection |
| `stone arch gaps` | List what the design lacks, each with the command that fixes it |
| `stone map [--write]` | Propose the code folder of each element without `code` |
| `stone scan` | Write draft components for code no element owns |
| `stone code explain <component>` | Show entry points, files, key symbols and dependencies |
| `stone code entries [<component>]` | Show where execution starts |
| `stone check` | Report files owned twice, or by no element |
| `stone instructions design` | Print the guide to write the design |

## Limits

- ⚠ A new project starts with an empty map. Run `stone map` and `stone scan`, then write the connections.
- Level 4 of C4 is not drawn. A component links to the code graph.
- C4 Dynamic flows stay as Mermaid in docs.

## Planned

- [[CS-203]]: no false `arch-drift` warnings from inferred edges.
- [[CS-184]]: the ownership check skips non-code files under named code roots.
- [[CS-239]]: `cli` lists `core` in `uses` but no code edge exists.
