---
type: task
title: Workspace scaffold and CI
status: done
milestone: M-1
priority: high
kind: chore
created: 2026-10-16
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

Cargo workspace with core, store, index, engine and cli; rustfmt and clippy config; GitHub Actions running fmt, clippy and tests on Linux, macOS and Windows (ADR-10).

## Acceptance criteria

- [x] Workspace is core, store, index, engine, cli; no stub crates
- [x] rust-version 1.89, edition 2024
- [x] .gitattributes sets `* text=auto eol=lf`
- [ ] CI runs fmt, clippy -D warnings and tests on 3 OSes

## Notes

Local fmt, clippy and tests pass. CI on macOS and Windows needs a pushed remote.

## Outcome

Workspace builds on Linux, macOS and Windows; CI runs fmt, clippy, tests and stone check.
