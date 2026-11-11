---
type: task
title: Item model for the OKF profile
status: done
milestone: M-1
priority: high
kind: feature
paths: [crates/core/src/]
created: 2026-10-23
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

Rework `carvestone-core` to the new PRD data model (ADR-3, ADR-4):
- Types: doc, plan, task, milestone, decision, reference, system, container and component.
- Common fields and relations: `links`, `depends_on`, `milestone`, `plan`, `components`, `paths`, `code`, `uses`, `libraries`.
- Task body sections.
- Provenance in OKF form: `generated` (by, at) and `verified` as a list of (by, at). OKF actor strings: `human:<id>`, `process:<id>`, `<producer>[/<version>]`. Trust tier derived as unverified, machine-confirmed or human-reviewed.
- The id is the path without `.md`; short ids (`CS-12`) resolve to it.
- Unknown keys are kept.

Remove what the new PRD drops: the attachment kind, the `reviewed` stamp and the context-bundle schema and renderers.

## Acceptance criteria

- [x] Every type parses from and serializes to frontmatter, with unknown keys kept
- [x] Short id ↔ path id resolution with canonical case
- [x] Trust tier derived from `verified`, as OKF defines it
- [x] Bundle schema, attachment kind and `reviewed` are gone

## Outcome

Item model for all nine types with OKF provenance, short ids and derived trust tier; old bundle model removed.
