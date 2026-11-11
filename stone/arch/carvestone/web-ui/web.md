---
type: component
title: carvestone-web
status: stable
created: 2026-10-26
generated: { by: 'human:yaroslav', at: 2026-10-26 }
parent: arch/carvestone/web-ui
code: [web/**]
uses: [arch/carvestone/stone/server]
technology: TypeScript
libraries: [react@19.3.0, vite@8.3.4, '@tanstack/react-router@1.170.42', '@tanstack/react-query@5.104.2', tailwindcss@4.3.3, vitest@5.0.3]
description: 'The browser UI served by stone serve: docs, items, board, architecture, code and timeline views over the server API.'
---

## Responsibilities

Renders the repository for people: reads through the server JSON API, edits through PATCH and POST, and refreshes from the SSE event stream. Built into web/dist and embedded in the binary by the server ui feature.

## Interfaces

## Gotchas

## Uses

- [[arch/carvestone/stone/server]]: Reads and edits items (HTTP)
