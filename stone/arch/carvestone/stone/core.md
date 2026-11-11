---
type: component
title: carvestone-core
description: Item model, ids, relations, provenance and body sections.
status: stable
technology: Rust
code: ['crates/core/**']
parent: arch/carvestone/stone
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
libraries: [ts-rs@12]
---

## Responsibilities

Defines item types, ids and short ids, fields, trust tiers, and splits a Markdown body into sections and `[[links]]`.

## Interfaces

`IdScheme` (parse, number, numbered_id, short, link_text), `Fields`, `Item`, `sections`, `set_section`, `check_box`, `body_links`.

## Gotchas

`IdScheme::number` accepts zero-padded names so `stone check` can flag them; `IdScheme::parse` always returns the canonical id.
