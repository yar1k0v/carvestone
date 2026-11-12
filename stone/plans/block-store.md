---
type: plan
title: Block store for stone items
status: draft
created: 2026-11-12T17:09:56+00:00
generated: { by: claude-code, at: '2026-11-12T17:09:56+00:00' }
description: Deferred design. Stone keeps items as compressed blocks in append-only chunks, with its own versioning, merge and graph.
tags: [design, store, deferred]
milestone: M-20
---

# Block store for stone items

Status: deferred design. The paragraph merge resolver comes first and is reused here.

## Problem

Agents in any harness read and grep `stone/*.md` directly instead of calling `target/debug/stone`. Permission rules differ per harness and do not stop this. Stone also has no document history of its own: it depends on git.

## Goals

- Store files are unreadable without stone, in every harness and on every OS.
- Each document has its own version history, independent of code commits.
- The link graph comes from the store, without a file scan.

## Out of scope

- A virtual file system (ProjFS, FUSE).
- Encryption. Stone holds any key, so it adds no protection.
- Stopping an agent that runs `git show` or stone itself.
- Own sync or a server. Git stays the transport for now.

## Model

Every stored thing is a block. A block is one whole file or one text delta of a Markdown file. A block never spans two chunks.

```
block  = header [kind · id · parents (0-2) · version · date · length] + payload + CRC32
chunk  = header [magic · format version] + block + block + …
```

| Part | Where | In git | Role |
|---|---|---|---|
| WAL | `.stone/wal` | no | Every write appends here first |
| Chunk | `stone/chunk-<hash>.bin` | yes | Sealed WAL. Immutable. Name is the hash of its content |
| Index | `.stone/index.db` | no | Existing SQLite + FTS5 index. Rebuilt from block headers |

- The block header holds the parent links. The index only caches them, so a lost index is rebuilt by a chunk scan.
- After 20 deltas, the next write stores a full file block. A read applies at most 20 deltas.
- Block ids are a hash of parents and content. Two machines that make the same merge get the same block.
- Closest known design: Bitcask (append-only data files, key index, merge, hint files).

## Flows

| Flow | Steps |
|---|---|
| Write | Take the lock, append the block to the WAL, fsync, update the index |
| Read | Index lookup → nearest full file block → apply deltas in version order |
| Sync | Seal the WAL into a new chunk. Merge documents with 2+ heads. Compact when there are more than 16 chunks |
| Open after crash | Check the WAL tail. Cut a block with a bad length or CRC32 |
| Compaction | Copy all blocks, same ids, sorted by id, into one chunk. Delete the old chunks. Output is deterministic |

## Merge

Chunks are never merged. Each contributor adds new chunk files, so git sees no conflict. After a git merge, a document can have 2 or more heads.

1. Find the nearest common ancestor of the heads through the parent links.
2. Diff each head against that ancestor, paragraph by paragraph (scanners in `crates/core/src/section.rs`).
3. Apply the merge rules of the stone merge resolver.
4. Write one merge block whose parents are the heads.

| Change | Result |
|---|---|
| One head changed a paragraph | Take it |
| Two heads inserted at the same place | Keep both, in write-time order |
| Two heads changed the same paragraph | Conflict. `stone check` fails until `stone resolve` writes a new block |

Contributors run `stone sync` on their branch before a PR. CI runs `stone check`, which fails while a document has 2 heads.

## Rules

- ⚠ Stone never appends to a chunk that is already in git.
- ⚠ `stone check` fails when the WAL holds writes that are not sealed at commit time.
- The crew rule "minimal diff, CRLF stays CRLF" applies to the decoded file, not to chunk bytes.

## Trade-offs

| For | Against |
|---|---|
| Hidden in every harness | Two histories: git for code, stone for documents |
| History per document | GitHub PR diffs show binary files |
| Graph and versions in one place | Contributors must run `stone sync` before a PR |
| Path to a standalone product | Stone owns crash recovery, compaction and format upgrades |

## Open questions

| Question | Owner |
|---|---|
| How often do agents read `stone/` directly? Count Read, Grep and `cat` calls on `stone/` in session transcripts | Lead |
| Delta format: line diff (`similar`, readable) or fossil-delta (smaller) | Lead, in the trial |
| Does a merge after compaction on another branch resolve all block ids? | Trial |

## Next

1. Paragraph merge resolver for today's Markdown store (a git merge driver).
2. Trial: format, write, read, crash recovery, one 2-head merge. Decide after the trial, based on code size.
