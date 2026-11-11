---
type: decision
title: Rust single binary with an embedded React UI
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

One download, offline, no runtime. The UI needs a board, a map and an editor.

## Decision

Rust workspace (edition 2024, Rust 1.89). The UI is React + Vite + shadcn/ui, built once and embedded with `rust-embed`. `axum` serves the HTTP API and live updates over SSE. Types are shared through `ts-rs`.

## Consequences

The UI build is a release step and the binary stays self-contained.
