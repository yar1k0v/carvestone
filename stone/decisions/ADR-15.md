---
type: decision
title: Rewrite the Rust tests as feature tests
status: accepted
created: 2026-11-05
generated: { by: claude-code, at: 2026-11-05 }
---

## Context

## Decision

Replace the Rust test suite with feature tests, in one cutover. The owner chose this after the mutant baseline showed the import tests caught 390 of 434 viable mutants, and 1,194 tests was more than the product needs.

- A test answers one question: does this feature work as expected?
- Each feature gets one test per scenario in its task, and one table test for all its wrong input and error cases.
- New tests live in `crates/<crate>/tests/features/`: `main.rs`, `support.rs` for helpers, and one file per feature. They use nothing from `tests/it`, and they do not port or read old tests.
- Proof is a hand-made break of the code once per scenario. No mutant runs.
- Old tests go in one step, once the new suite passes.
- Kept: `bench_search`, `bench_code`, `output_cost`, and `ts_export`, which generates the web types for CI. Web tests are out of scope.

## Consequences

The old tests went before CS-144, CS-145 and CS-146 closed, and the owner accepted that on 2026-11-05. The test for `check --syntax` listing a parse error went with them, so CS-144 adds a feature test for it.
