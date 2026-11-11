---
type: task
title: Engine and CLI for items
status: done
milestone: M-1
priority: high
kind: feature
depends_on: [CS-4]
paths: [crates/engine/src/, crates/cli/src/]
created: 2026-10-23
generated: { by: 'human:yaroslav', at: 2026-10-25 }
closed: 2026-10-25
---

## Description

`carvestone-engine` holds `Repo { store, config }` and the operations. The CLI only parses and renders (ADR-10).

Commands:
- `stone init`;
- `stone new <type>`, `edit`, `get <id>[#section]`;
- `stone list` with filters for type, status, milestone, component;
- `stone task move <id> <status>` and `stone task next` (dependencies done, by priority);
- `stone link <id> <id>`.

Every read supports `--json`; every write supports `--dry-run`. The author of a write comes from `STONE_AGENT`, falling back to `human:<git user.name>`.

## Acceptance criteria

- [x] Every command works on this repo's `stone/`
- [x] `get CS-2#acceptance-criteria` prints only that section
- [x] Running the same edit twice gives no diff

## Outcome

Engine and CLI for init, new, edit, get, list, link, task move and next.
