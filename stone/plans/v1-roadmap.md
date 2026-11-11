---
type: plan
title: Roadmap to v1
status: done
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-07 }
description: Order, cross-cutting decisions and task lists for M-2 to M-7.
milestone: M-2
---

## Summary

v1 is M-1 to M-7 with the full PRD scope. Coding is serial (one editing agent, one cargo process); the parallel lane is scouting, fixtures and the next milestone's FR/AC plan, written one milestone ahead in the style of `plans/m1-store-cli`. Tasks are created with `stone new task` when their milestone starts.

Order: close M-1, M-2 search, M-5 agents, M-3 code graph, M-4 git, M-6a server, M-6b UI, M-7 Groma importer and the Freightline switch. M-5 comes early because it is a thin layer over engine calls and agents get value at once; M-3 and M-4 each add their MCP tool. The Freightline switch waits for the UI (fumadocs replacement).

## Cross-cutting decisions

- Crates: core, store, index, graph and git are leaves (index, graph and git depend on core only; graph and git never touch SQLite). Engine is the only crate that joins them; cli, mcp and server call engine only. The UI is `web/` at the repo root, not a crate.
- One index file `.stone/index.db`, WAL, `meta.schema_version`; a mismatch deletes and rebuilds (ADR-1), no migrations. All tables live in `crates/index/src/schema.rs`: sections, fts, links, files (M-2); symbols, edges (M-3); commits, commit_refs, blame (M-4).
- Reindex by content hash is `engine/src/refresh.rs`: mtime+size fast path, then blake3, diff against `files`, one `BEGIN IMMEDIATE` transaction. CLI reads call it lazily, the `serve` watcher calls the same function. The index is derived, so the store lock is not taken.
- Errors: engine returns `engine::Error` with a plain-sentence Display; the CLI adds `✗` and exit codes, MCP returns the sentence as a tool error, HTTP returns `{error, kind}`. Every query returns one serde struct shared by `--json`, MCP and HTTP (ts-rs derives behind an engine feature `ts`). Shared hit shape `{id, anchor, path, lines, kind, snippet, exactness?}`. `--budget` lives in the engine and counts chars/4.
- Benchmarks live in `bench/` outside `code_roots`. Freightline stays out of this repo: CI runs frozen synthetic or open-source fixtures with expected results; Freightline recall and timing are measured locally and recorded in the milestone file.
- Resolver pipeline for every language: declare qualified symbols per file, build a per-file import table, resolve references in the order local scope, imports, same namespace or module, project-wide name match. `exact` means resolved to one definition without the name-only fallback; the rest is `inferred`. The 90% usages bar applies to all four languages, each with its own sample.
- Web: `web/` uses pnpm, Vite and the PRD stack, builds to `web/dist`; `server` embeds it with rust-embed behind feature `ui` (on for release), a placeholder page otherwise, so `cargo test --workspace` never needs Node. ts-rs bindings are exported to `web/src/api/types/` by an engine test and CI fails on a diff. A separate CI `web` job runs typecheck, lint, vitest and build.
- No repo-specific sub-agent for now; CLAUDE.md carries the invariants. After M-5, register `stone mcp` in the project MCP config (the user edits `.mcp.json`).

## M-1 close

Commit the pending CI, toolchain, lint and test changes; move CS-1 to CS-6 to done with outcomes; do CS-7 (dogfood). Fold the duplicated lint tables in core, store and engine into `[workspace.lints]` plus the three extra denies.

## M-2 Search

Done when: recall@5 ≥ 0.9 on `bench/search` in CI, `stone search q --budget 200` gives about 40-token lines, `get --json` shows backlinks, editing one file reindexes only that file, a schema bump rebuilds silently. Risk: concurrent refresh during writes.

1. Backlog.md importer for tasks and docs (`engine/src/import/backlog.rs`): needed first so the Freightline fixture exists in stone format. Golden-file tests; running twice gives no diff.
2. Index schema v2: meta, files, sections, weighted fts, links; path ids; drop-rebuild (`crates/index/src`).
3. Section units from items through the `core/src/section.rs` scanners (anchor, level, line range).
4. Content-hash refresh (`engine/src/refresh.rs`), hooked into `Repo` reads. High: second run writes nothing, one edit reindexes one file, two processes refreshing at once.
5. Link and backlink extraction into `links`; backlinks in `get` text and JSON.
6. Query: BM25, filters, `--from`/`--component` boost, budget (`engine/src/search.rs`).
7. `stone search` CLI with `--json`.
8. Bench harness: `bench/search/queries.yml`, `engine/tests/bench_search.rs`; latency on 5,000 generated sections, asserted loosely on Linux only.

## M-5 Agents

Done when: an MCP client lists tools and calls search/get/create; workflow resources are served; `instructions` points at Docs MCP for component `libraries`. Risk: rmcp MSRV against 1.89.

1. `crates/mcp`: rmcp stdio, tools search/get/list/create/update/link, `stone mcp`. High (dependency).
2. One guide text in the engine shared by `stone instructions` and the MCP resources overview, plan, do, finish.
3. Component `libraries` shown by `instructions` and `get` with a `search_docs` hint.
4. E2E: spawn `stone mcp`, run initialize, tools/list and a call (`crates/mcp/tests`).

## M-3 Code graph and architecture

Done when: `uses` recall ≥ 0.9 on each language's sample; `code find/uses/deps/impact/path/explain` return budgeted hits marked exact or inferred; `[[code:...]]` resolves in check and backlinks; check reports arch drift; one changed file reindexes in under 1 s; modules and draft components are produced. Risk: re-resolving inferred edges when names appear or disappear.

1. `crates/graph` and the C# grammar: project, namespace, file, type, method nodes with signature, doc, lines. High (grammar, MSRV, binary size).
2. Index schema v3: symbols, edges (kind, exact), symbols in fts.
3. C# resolver (namespaces, usings, ProjectReference, partial classes; member calls exact when the receiver type is declared). High.
4. Graph in refresh: per-file update plus re-resolution of dependents by name, same transaction. High.
5. `code find/uses/deps` in SQL plus CLI.
6. `code impact/path/explain` via petgraph, joined to components, open tasks and docs.
7. `[[code:symbol]]` and `[[code:path:a-b]]` in check (currently skipped), backlinks, symbols in search.
8. Graph dependencies between components and drift against `uses`, in check and in `get` for arch items.
9. Rust extraction and resolver (Cargo.toml, mod tree, `use` paths); dogfood bench against rust-analyzer references. High.
10. TS/JS: relative and tsconfig-paths resolution, import/export. Python: packages, pyproject, import/from. Member and attribute calls are inferred. Each reaches the 90% bar with its own sample.
11. Louvain modules within project and folder boundaries; `explain` uses them.
12. `stone scan`: draft components from clusters; writes drafts and its own fields only; idempotency test.
13. Bench harness `engine/tests/bench_code.rs` with `bench/code/<lang>/expected_uses.yml`; the user captures IDE find-usages once per sample, the scout prepares the samples.
14. MCP `code` tool.

## M-4 Git and history

Done when: `why file:line` shows commits, tasks, component, docs and falls back through blame, component, open tasks; `history ADR-7` shows diffs; `timeline --since` mixes kinds; a task's changed files show in `get`; stale items are flagged. Risk: blame speed on Windows and large files, shallow clones in CI.

1. `crates/git`: CLI wrapper (log with trailers, `blame -p`, show); parser tests on canned output.
2. Commit cache keyed by sha in the index (schema v4), refreshed from HEAD.
3. `stone commit-msg` and an opt-in hook install and check.
4. `why`, 5. `history`, 6. `timeline`: engine module plus CLI with `--json` each.
7. Task changed files (trailer commits plus working diff) in `get` and `list --json`.
8. Staleness: `stale_after` passed, or code under the item's components changed N commits after `verified`; check warning and `list --stale`.
9. MCP `why` and `history` tools.

## M-6a Server

Done when: `stone serve` answers every route, a file edit pushes an SSE event, a PATCH gives the same minimal diff as the CLI. Risk: watcher and lazy refresh racing.

1. `crates/server`: axum read routes (`/api/items`, `/api/items/{id}`, `/api/search`, `/api/code/{op}`, `/api/why`, `/api/history/{id}`, `/api/timeline`) and the placeholder page.
2. Write routes (`PATCH /api/items/{id}`, `POST /api/items`) through the engine edit path.
3. `notify` watcher feeding refresh and `GET /api/events` (SSE). High.
4. ts-rs export test and the bindings diff check in CI.

## M-6b Web UI

Done when: docs site with sidebar, Mermaid and backlinks; board drag moves status in git; search; item pages with history and provenance; milestones and plans; architecture map; graph explorer; timeline; editing everywhere as file writes. Risk: scope; one agent batch per 2-3 views.

1. `web/` scaffold with pnpm, CI web job, rust-embed `ui` feature, release wiring; add a `web/` arch component and update the CLAUDE.md check list.
2. Docs site. 3. Item pages. 4. Board (dnd-kit). 5. Milestones and plans. 6. Search. 7. Architecture map (React Flow, drift). 8. Code graph explorer. 9. Timeline. 10. Editing: field forms and CodeMirror sections on every item type.

## M-7 Importers and switch-over

Done when: Groma elements become `arch/` items with code globs and `uses`; Backlog import covers milestones and decisions; re-import gives no diff; Freightline passes `stone check` with no old tool left in its repo.

1. Backlog importer, the rest: milestones, decisions, acceptance criteria/plan/notes mapping, prefix.
2. Groma importer: `groma.code` to globs, relationship tables to `uses`, flows and drafts as sections. High (format variance).
3. `--dry-run` and idempotency tests for both.
4. Freightline switch checklist as a doc; the run itself is the user's call.

## Open

- The user captures IDE find-usages for each language sample (C#, Rust, TS/JS, Python) and picks the open-source or synthetic fixtures.
- Check rmcp, tree-sitter and grammar, axum and ts-rs MSRV against Rust 1.89 before M-5 and M-3.
- Whether Freightline keeps its knowledge in Backlog.md, Groma or fumadocs (the Backlog-first importer assumes Backlog.md).
