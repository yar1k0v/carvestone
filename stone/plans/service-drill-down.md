---
type: plan
title: Service drill-down
status: draft
created: 2026-10-27
generated: { by: claude-code, at: 2026-11-07 }
milestone: M-12
links: [CS-91, CS-92, CS-93, CS-94, CS-96, CS-97, CS-100, CS-101, CS-103, CS-104, CS-105, CS-106]
---

## Goal

An architect or an agent opens a service and sees how it is built: inside view, entry points, packages, shared libraries, patterns, topics, and where code and diagram disagree. Replaces the Code map tab, which shows boxes without links on a real product (Freightline: 16 elements, 0 code edges, because its service items name no code).

## Decisions

Settled with the product owner on 2026-10-27.

- D1 Audience: the architect and agents. Structure, boundaries, patterns.
- D2 Mapping: stone proposes each service's code from its project file (.csproj, package.json, pyproject) by name; the owner confirms once and it is written to the service item's `code`.
- D3 Agents: the same view in the CLI (`stone code explain <service>`) and the UI, from one engine answer.
- D4 Patterns: API surface, messaging, data access, app structure (handlers, workers, DI registrations, layers).
- D5 Languages: C#, TypeScript and Python patterns together. Dart/Flutter later.
- D6 Shared projects show as libraries a service uses, each with its own view.
- D7 Messaging includes topic and queue names from code and config; computed names are marked inferred. Services link through topics.
- D8 The Code map tab goes. The drill-down starts from the hand-drawn diagram; without one, stone draws its services with their code and topic links.
- D9 Done: the owner reviews Processing, Api, admin portal and notifier in Freightline and agrees; every pattern lists examples.
- D10 Drift both ways: what the code does that the diagram lacks, and what the diagram claims the code never does.
- D11 Detected facts are computed from the index, never written to items.
- D12 Two milestones: M-12 service view and drill-down; M-13 patterns, topics and drift.

## Assumptions

- The inside view groups a service by its top folders or namespaces, with dependency arrows and counts between them.
- Dart (Flutter mobile) and Terraform repos show as boxes only.
