---
type: component
title: carvestone-server
status: stable
created: 2026-10-25
generated: { by: 'human:yaroslav', at: 2026-10-26 }
code: [crates/server/**]
uses: [arch/carvestone/stone/engine]
parent: arch/carvestone/stone
technology: Rust
libraries: [axum@0.8.9, notify@8, rust-embed@8, tokio@1]
description: The HTTP server behind stone serve; it exposes engine reads and writes as a JSON API with an event stream and the embedded web UI.
---

## Responsibilities

Maps HTTP requests to engine calls, streams file changes as server-sent events and serves the built web UI.

## Interfaces

`stone serve [--port 4747] [--host 127.0.0.1] [--allow-host <NAME>]... [--open] [--no-watch]`. `--allow-host` repeats and adds a `Host` name to accept. Bound to 0.0.0.0 or ::, the server accepts loopback, any IP-literal host and the listed names; on a specific bind it accepts loopback, the bound address and the listed names. Read routes under `/api` (items, search, code, config, arch graph, why, history, timeline), `PATCH /api/items/{id}` with an `If-Match` header or `if_match` field, `POST /api/items`, and `GET /api/events` (SSE: items, code, error). Feature `ui` embeds `web/dist`.

## Gotchas

A write naming a foreign `Origin` and a request with a foreign `Host` get 403; a non-loopback bind has no authentication. The watcher must keep `Access(Close(Write))`, which is how Linux reports a finished write.

## Uses

- [[arch/carvestone/stone/engine]]: Serves every read and write
