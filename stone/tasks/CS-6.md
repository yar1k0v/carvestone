---
type: task
title: stone check and renumber, run in CI
status: done
milestone: M-1
priority: high
kind: feature
depends_on: [CS-5]
paths: [crates/engine/src/, crates/cli/src/, .github/workflows/]
created: 2026-10-23
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

`stone check` validates:
- OKF conformance and field types;
- status, kind and priority values against config;
- that links, relations and `#anchors` resolve;
- duplicate ids;
- that done tasks have a `## Outcome`;
- architecture `code` globs: files owned by two elements, or by none within `code_roots`.

Errors are actionable ("ADR-11 not found. Did you mean ADR-1?") and the exit code is non-zero. `stone renumber <path>` fixes a duplicate id and updates links to it. CI runs `stone check`.

## Acceptance criteria

- [x] This repo passes; each rule has a failing fixture
- [x] Did-you-mean suggestions for unknown ids
- [x] CI step runs `stone check`

## Outcome

stone check and renumber implemented; check runs in CI and is clean on this repo.
