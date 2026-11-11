---
type: decision
title: Publish from a fresh history with client names removed, under MIT OR Apache-2.0
status: accepted
created: 2026-11-07
generated: { by: claude-code, at: 2026-11-11 }
---

## Context

The repo has no remote and 73 commits. 80 tracked files name Freightline or Fleetwise, mostly in `stone/`, and 3 of them are in `crates/` or `web/`. The repo root has no LICENSE file, and `README.md` is empty.

## Decision

- Replace the client names in all tracked files.
- Publish a new repo with one starting commit.
- Keep `stone/` public as a live example of the tool.
- The public repo is the working repo. The old history and the client bench store stay in a private backup folder (owner, 2026-11-11).
- License: MIT OR Apache-2.0.

## Consequences

⚠ Publishing is irreversible: public content stays in every clone. Run the name search on the new tree before the first push. The owner approves the push.

- The count of 80 files predates the release work. Repeat the search at publish time.
- Rejected: code only, because the public repo loses its example.
- Rejected: full history, because client names stay public in every clone.
