---
type: decision
title: Own schema as an OKF v0.2 profile
status: accepted
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Context

We need our own fields (relations, task sections, architecture) without locking the data into one tool. OKF v0.2 requires only `type`, allows extra keys, and defines provenance and freshness.

## Decision

- `stone/` is a valid OKF bundle, with `okf_version` in the root `index.md`.
- OKF names are used where OKF has them: `type`, `title`, `description`, `tags`, `status`, `generated`, `verified`, `stale_after`, `resource`.
- Our own keys are extensions: `links`, `depends_on`, `milestone`, `plan`, `components`, `paths`, `code`, `uses`.
- Unknown keys are kept on every write.
- Not compatible with Backlog.md or Groma.md; formats are designed for agents first.

## Consequences

Other OKF tools can read the bundle. Importers for Backlog.md and Groma.md can come later.
