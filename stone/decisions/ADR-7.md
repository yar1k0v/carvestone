---
type: decision
title: Search returns sections, ranked by BM25 plus links, under a token budget
status: accepted
created: 2026-10-23
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

Agents pay for every token. Whole-file results waste context, and similarity alone isn't deterministic.

## Decision

- Index units are sections, split by heading, each keeping its id, anchor and line range.
- Ranking is FTS5 BM25 with title and heading weights, plus a boost for items linked to the given context (`--from`, `--component`).
- Output is about 40 tokens per hit, capped by `--budget`. The agent expands with `get <id>#section`.
- Embeddings come later and only fill gaps.
- A fixed Freightline query set measures recall@5, tokens and latency in CI.

## Consequences

Accuracy is measured, not assumed. Results are deterministic for a given repo state.
