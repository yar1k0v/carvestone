---
type: doc
title: Agent instructions
status: draft
created: 2026-11-05
generated: { by: claude-code, at: 2026-11-05 }
description: Project values that the crew toolboxes read.
---

## Profile

- stone: `target/debug/stone.exe` from the repo root; never on PATH. After a code change: `cargo build --bin stone`.
- ids: tasks `CS-n`, milestones `M-n`, decisions `ADR-n`, design `arch/carvestone/<container>[/<component>]`.
- checks: `.claude/rules/crew.md#checks`
- tests: `.claude/rules/testing.md` decides patch, add or none. Rust: the crate's existing test files, usually `crates/<crate>/tests/it/<topic>.rs`. Web: `*.test.ts(x)` beside the code.
- protected: `stone/` and `.stone/` only through the stone binary; `web/src/routeTree.gen.ts` is generated; never commit `docs/agent-scratch/`.
- layer order: `core` → `store`, `git`, `graph`, `index` → `engine` → `cli`, `mcp`, `server` → `web`.
- one heavy command at a time: every cargo process.
- UI done: the screen matches its board on the design canvas (ADR-14), in light, dark and phone views.
- commits: only when the owner asks; the trailer comes from `target/debug/stone.exe commit-msg --task <id>`; never push.
