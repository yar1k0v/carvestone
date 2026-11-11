---
type: plan
title: M-6 server and web UI requirements
description: The stone serve HTTP API with live events (M-6a) and the embedded React UI (M-6b), as checkable behaviour. Serves the M-6 task list.
status: done
milestone: M-6
links: [M-6, ADR-1, ADR-2, ADR-10, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-07 }
---

## Summary

At the end of M-6 `stone serve` opens a local site from the one binary: docs with sidebar, Mermaid and backlinks; a board where a drag moves status in git; milestones and plans; search; item pages with history and provenance; an architecture map with drift; a code graph explorer; a timeline; and editing of fields and sections on every item type. Part M-6a is the axum API, the file watcher and the type export; part M-6b is the `web/` app. Every edit is the same minimal-diff file write the CLI makes. The Freightline switch and importers are M-7.

## Background

- ADR-10: the server calls the engine only. ADR-2: React + Vite + shadcn/ui built once and embedded with `rust-embed`, `axum` with SSE, types shared by `ts-rs`. ADR-1: files are the truth; the watcher only triggers a reindex.
- Roadmap: `web/` at the repo root (pnpm, Vite, PRD stack), builds to `web/dist`; `server` embeds it behind feature `ui` (on for release) with a placeholder page otherwise, so `cargo test --workspace` never needs Node; ts-rs bindings go to `web/src/api/types/` from an engine test and CI fails on a diff; a separate CI `web` job runs typecheck, lint, vitest and build. Routes: `/api/items`, `/api/items/{id}`, `/api/search`, `/api/code/{op}`, `/api/why`, `/api/history/{id}`, `/api/timeline`, `/api/events`, `PATCH /api/items/{id}`, `POST /api/items`.
- Engine returns one serde struct per query shared by `--json`, MCP and HTTP; HTTP errors are `{error, kind}`. Refresh (M-2) is lazy on reads; the watcher calls the same function. Writes take the store lock (M-1 invariant).
- No UI tests run a browser in CI; the user verifies views from click-lists in each task.

## Goals

- Read and edit a whole Freightline-sized store in the browser with no fumadocs and no IDE.
- The API is the engine over HTTP: same structs, same errors, same diffs.
- `cargo test --workspace` and `cargo run -q --bin stone -- check` stay Node-free.

## Users and scenarios

- A person runs `stone serve`, opens the printed URL, drags a task from todo to in-progress, and `git diff` shows one changed `status:` line. Failure path: dragging to done without `## Outcome`; the card snaps back and a toast shows the engine sentence.
- A second editor changes a file while the page is open; the page updates through SSE. Failure path: the person saves a section based on the old text; the server answers 409 "tasks/CS-4 changed on disk; reload" and nothing is written.
- An agent edits through MCP (another process) while the UI is open; the server's watcher sees the file change on disk and the board refreshes with no reload.
- Failure path: port 4747 is taken; `stone serve` exits 1 with "port 4747 is in use; pass --port".

## Scope

- In: `crates/server`, `stone serve`, read and write routes, SSE with a `notify` watcher, ts-rs export and CI diff, `web/` with ten views, CI `web` job, release wiring, a `web` arch component.
- Out: auth and multi-user (localhost only), file store and uploads, static site export, importers (M-7), mobile layouts, offline mode, UI end-to-end tests in a browser.

## Functional requirements

### FR-1 Server and hardening (M-6a)

`stone serve [--port N] [--host H] [--open] [--no-watch]` binds `127.0.0.1:4747` by default, prints the URL, and runs until Ctrl+C. A non-loopback `--host` prints "no authentication; anyone on this network can edit". Every request must carry a `Host` header equal to the bound address, `localhost` or an IP literal of loopback; otherwise 403 `{error: "host not allowed", kind: "forbidden"}` (DNS rebinding). Write requests with an `Origin` header that differs from the `Host` get 403 (cross-site). Errors map: not found 404 `not_found`; validation 422 `invalid`; stale `if_match` 409 `conflict`; bad query 400 `bad_request`; anything else 500 `internal` with the sentence. `engine::Error::kind()` returns the kind (new).

Acceptance criteria:
- AC-1.1 Given a taken port, when `stone serve` starts, then exit 1 with "port 4747 is in use; pass --port".
- AC-1.2 Given a request with `Host: evil.example`, when sent, then 403 `forbidden`.
- AC-1.3 Given a PATCH with `Origin: http://evil.example`, when sent, then 403 and no file changes.
- AC-1.4 Given `GET /api/items/tasks/CS-999`, when sent, then 404 `{error: "CS-999 not found", kind: "not_found"}`.

### FR-2 Read routes (M-6a)

All return the engine structs as JSON. `GET /api/items?type&status&milestone&component&stale` (list rows; every row carries `etag`, the same value `get` returns, also in `stone list --json`); `GET /api/items/{id}` with `{id}` a path id or short id (wildcard path), optional `?section=anchor`, returns the get result with backlinks, `changed_files`, `libraries`, `stale` and `etag` (blake3 of the file), also sent as `ETag`; `GET /api/search?q&from&component&type&status&milestone&limit&budget`; `GET /api/code/{op}?target&to&depth&exact&budget` with op in `find, uses, deps, impact, path, explain, modules`; `GET /api/why?path&line`; `GET /api/history/{id}?limit&diff`; `GET /api/timeline?since&until&component&kind&limit`; `GET /api/config` (statuses and priorities per type, task prefix, field specs per type); `GET /api/arch/graph` (components, curated `uses`, code edges, drift findings). Reads refresh lazily.

Acceptance criteria:
- AC-2.1 Given this repo, when `GET /api/items?type=task&milestone=M-1` runs, then the JSON equals `stone list --type task --milestone M-1 --json`.
- AC-2.2 Given `GET /api/items/CS-2?section=acceptance-criteria`, when sent, then only that section's text and range return, with an `etag`.
- AC-2.3 Given `GET /api/search?q=minimal+diff&budget=200`, when sent, then the body equals `stone search --json` for the same arguments.
- AC-2.4 Given `GET /api/code/uses?target=X`, when X is ambiguous, then 422 with the candidate list in `error`.
- AC-2.6 Given a task file, when `GET /api/items?type=task` and `GET /api/items/{id}` are sent, then the row's `etag` equals the get result's `etag`.
- AC-2.5 Given `GET /api/config`, when sent, then it lists `todo`, `in-progress`, `done` among task statuses and a field spec for each item type.

### FR-3 Write routes (M-6a)

`PATCH /api/items/{id}` takes `{set?, unset?, section?: {name, text}, check_ac?, dry_run?, if_match?}` and runs the engine edit path (task status through `task move` rules); with `if_match` different from the file's current etag the answer is 409 and nothing is written. It returns `{id, changed, etag, text?}`. `POST /api/items` takes `{type, title, slug?, fields?, sections?}` and returns 201 `{id, path, etag}`. The author is `STONE_AGENT`, else the git user, as in the CLI. Identical repeated PATCH returns `changed: false`.

Acceptance criteria:
- AC-3.1 Given CS-2, when `PATCH {set: {priority: "low"}}` is sent and the same via `stone edit`, then the resulting bytes of the two copies are equal.
- AC-3.2 Given the same PATCH twice, when sent, then the second returns `changed: false` and the file is byte-identical.
- AC-3.3 Given `if_match` of an old etag, when sent, then 409 `conflict` with "tasks/CS-2 changed on disk; reload" and no write.
- AC-3.4 Given `set status=done` on a task without `## Outcome`, when sent, then 422 with the M-1 sentence and no write.
- AC-3.5 Given `POST {type: "task", title: "X"}`, when sent, then 201 and the file exists with `status: todo`.

### FR-4 Watcher and events (M-6a)

Unless `--no-watch`, `notify` watches `stone/` and `code_roots` (ignoring `.git/`, `.stone/`, `target/`, `node_modules/`, `web/dist/`), debounced 200 ms. Events are built from the watched paths of the batch, not from what the watcher's own refresh changed (another process may have indexed first, and `RefreshReport` has only counts): a path under `stone/` ending in `.md` maps to its path id and, when numbered, its short id (`engine::ids_for_paths`, deleted files included); other paths under `code_roots` go to `code`. The batch then calls refresh and, after it returns, sends on `GET /api/events` (SSE) `event: items` with `{ids: [...]}` and `event: code` with `{paths: [...]}`. A refresh error sends `event: error` with the sentence and the next batch retries. A comment line `: ping` every 15 s. A client that disconnects is dropped without affecting others. A burst of 100 edits yields at most a few events, never one per file save when saves land in the same window.

Acceptance criteria:
- AC-4.1 Given an SSE client, when a stone file is edited on disk, then an `items` event with its id arrives within 2 s.
- AC-4.2 Given a code file edit, when it happens, then a `code` event arrives and `GET /api/code/find` sees the new symbol.
- AC-4.3 Given a PATCH through the API, when it completes, then the same client receives an `items` event.
- AC-4.4 Given `--no-watch`, when files change, then no event is sent, and reads still see the change (lazy refresh).
- AC-4.6 Given another process refreshed the index before the watcher's batch ran, when the batch fires, then the `items` event still carries the edited ids.
- AC-4.7 Given a deleted item file, when watched, then its id is in an `items` event.
- AC-4.5 Given 20 saves in 100 ms, when watched, then at most 2 events arrive and the last state is indexed.

### FR-5 Embedded UI and type export (M-6a)

With feature `ui`, `rust-embed` serves `web/dist` at `/` with an SPA fallback to `index.html` for non-`/api` GETs and long-cache headers for hashed assets; the build fails with "web/dist is missing; run `pnpm --dir web build`" when the folder is absent. Without it `/` serves a small placeholder page naming that command. An engine test behind feature `ts` writes ts-rs bindings for every API struct into `web/src/api/types/`; CI runs it and fails on `git diff --exit-code web/src/api/types`.

Acceptance criteria:
- AC-5.1 Given no `ui` feature, when `GET /` is sent, then 200 HTML containing "pnpm --dir web build".
- AC-5.2 Given `ui` and a built `web/dist`, when `GET /docs/prd` is sent, then `index.html` returns.
- AC-5.3 Given an API struct field added without regenerating, when CI runs the export and diff, then CI fails.
- AC-5.4 Given `GET /api/nope`, when sent, then 404 JSON, not the SPA page.

### FR-6 App shell (M-6b)

`web/` is a pnpm Vite app (React, TypeScript strict, Tailwind, shadcn/ui, TanStack Router and Query) with a left nav (Docs, Board, Milestones, Plans, Search, Architecture, Code, Timeline), a top bar with search, theme (light, dark, system; remembered locally) and connection state. The API client is typed from `src/api/types`. One SSE hook invalidates Query caches by event (`items` invalidates those ids and lists; `code` invalidates `code` and `arch` queries) and reconnects with backoff; the bar shows "offline" when the server is gone. Fonts are bundled; the app makes no request outside the origin. Route-level code splitting keeps Mermaid, CodeMirror and React Flow out of the first bundle.

Acceptance criteria:
- AC-6.1 Given an `items` event for `tasks/CS-4`, when received, then the item and list queries for it refetch (vitest with a fake EventSource).
- AC-6.2 Given a closed stream, when it drops, then "offline" shows and a reconnect is attempted with growing delay.
- AC-6.3 Given `pnpm --dir web build`, when it ends, then the initial JS is under 400 KB gzipped.

### FR-7 Docs site (M-6b)

`/docs/*` renders doc items: sidebar from the folder tree of `docs/**` ordered by `order` then title, folders collapsible; GFM markdown; heading anchors computed with the same GitHub slug rules as the engine (duplicates get `-1`); Mermaid fences render as diagrams (theme-aware, lazy); `[[id]]` and `[[id#anchor]]` and `.md` links become router links, broken ones are marked; `[[code:...]]` becomes a chip linking to the explorer; a right-hand table of contents; a Backlinks panel from the get result; a stale banner when `stale` is set.

Acceptance criteria:
- AC-7.1 Given headings `B`, `B`, when rendered, then anchors are `b` and `b-1` (vitest against the engine's fixture list).
- AC-7.2 Given `[[ADR-3#decision]]`, when rendered, then the link targets `/items/decisions/ADR-3#decision`.
- AC-7.3 Given a mermaid fence, when the page opens, then an SVG renders; a syntax error shows the error text, not a blank.
- AC-7.4 Given a doc with backlinks, when opened, then the panel lists them with kind and line.

### FR-8 Item pages (M-6b)

`/items/*` shows any item: header (id, title, status badge, trust badge derived from `verified`), field table, sections with anchors, links and backlinks, tabs History (entries with diffs from `/api/history`, section-filtered by anchor), Provenance (`generated`, each `verified` entry), and for tasks Changed files; for arch elements the code dependencies and drift; for components the libraries line with the `search_docs` hint; a Stale banner. Short ids redirect to path ids.

Acceptance criteria:
- AC-8.1 Given an item with two `verified` entries, when opened, then Provenance lists both and the trust badge reads `human-reviewed` or `machine-confirmed` as the engine says.
- AC-8.2 Given `/items/CS-2`, when opened, then the URL becomes `/items/tasks/CS-2`.
- AC-8.3 Given an unknown id, when opened, then a not-found page shows the engine's did-you-mean.
- AC-8.4 Given the History tab, when opened, then each commit shows sha, date, author and a collapsible diff.

### FR-9 Board (M-6b)

`/board` shows tasks in columns from `/api/config` task statuses, cards with id, title, priority, assignee and milestone; filters by milestone, component, text and assignee (kept in the URL). Dragging with `@dnd-kit` (pointer and keyboard) sends `PATCH {set: {status}, if_match: <the row's etag>}` and stores the reply's new etag on the card; the card moves optimistically and snaps back on any error with a toast of the sentence. Order inside a column is by priority then number.

Acceptance criteria:
- AC-9.1 Given a drag from todo to in-progress, when dropped, then one PATCH with `{status: "in-progress"}` and the row's `if_match` etag is sent and the card stays (vitest with a mocked client).
- AC-9.2 Given a 422 answer, when received, then the card returns to its column and the toast shows the sentence.
- AC-9.3 Given `?milestone=M-2`, when opened, then only M-2 tasks show.
- AC-9.4 Given a 409, when received, then the board refetches and the toast says to retry.

### FR-10 Milestones and plans (M-6b)

`/milestones` lists milestones with status, target and done/total tasks; its page shows the goal text and tasks grouped by status. `/plans` lists plans with status and milestone; a plan page renders the plan, its tasks (items whose `plan` is the plan) with status, and linked decisions.

Acceptance criteria:
- AC-10.1 Given 3 of 4 tasks done, when the list opens, then the milestone shows 3/4.
- AC-10.2 Given a plan with 5 tasks, when opened, then 5 task rows show with statuses.

### FR-11 Search (M-6b)

Ctrl+K (and `/`) opens a palette; `/search` is the full page. Queries are debounced 200 ms, send `budget` 400 and filters (type, status, milestone, component), and show hit lines with kind icon, id, anchor, line range and snippet with highlights. Enter opens the item at the section (symbol hits open the explorer). Empty result shows the query and a hint to broaden filters.

Acceptance criteria:
- AC-11.1 Given typing "minimal diff", when 200 ms pass, then exactly one request fires.
- AC-11.2 Given a hit `docs/prd#search`, when chosen, then the router goes to `/docs/prd#search`.
- AC-11.3 Given a `symbol` hit, when chosen, then the explorer opens with that symbol.

### FR-12 Architecture map (M-6b)

`/architecture` draws C4 elements from `/api/arch/graph` with React Flow and `dagre` layout: systems and containers as groups, components as nodes with technology; curated `uses` as solid edges, code edges as dashed with the count, drift edges red with a legend; draft components (from `stone scan`) drawn with a dashed border. Clicking a node opens a side panel (responsibilities, owned globs, libraries, drift) and a link to its item page; filters hide drafts or code edges.

Acceptance criteria:
- AC-12.1 Given a drift finding, when drawn, then that edge is red and the panel lists the finding text.
- AC-12.2 Given a draft component, when drawn, then its border is dashed.
- AC-12.3 Given "hide code edges", when toggled, then only `uses` edges remain.

### FR-13 Code graph explorer (M-6b)

`/code` has a symbol search box (`code/find`), a detail pane with signature, doc, backlinked docs and tasks, tabs Uses, Dependencies, Impact (components, tasks, docs), and a React Flow neighbourhood view (depth 1 to 3) where exact edges are solid and inferred dashed with a toggle to hide inferred; a Modules tab from `code/modules`; a Path form (`code/path`). Nodes open the source path with line range as text (no editor).

Acceptance criteria:
- AC-13.1 Given an ambiguous name, when searched, then the candidate list shows and choosing one loads it.
- AC-13.2 Given "hide inferred", when toggled, then dashed edges disappear.
- AC-13.3 Given an impact query, when shown, then components, tasks and docs list under their headings.
- AC-13.4 Given `path` with no route, when run, then "no path between A and B" shows.

### FR-14 Timeline (M-6b)

`/timeline` lists events from `/api/timeline` newest first, grouped by day, with kind chips (commit, task done, decision, doc), a since selector (7d, 30d, 90d, all), a component filter and "load more" that raises the limit. Items link to their pages; commits show sha and subject.

Acceptance criteria:
- AC-14.1 Given the kind chip "decision" only, when set, then the request has `kind=decision`.
- AC-14.2 Given "load more", when pressed, then the request limit doubles and rows append.

### FR-15 Editing (M-6b)

Every item page has Edit mode. Field forms come from the field specs: enums as selects, lists as chips with a picker that searches items, relations validated against `/api/search`, dates as date inputs; saving sends only changed keys in one PATCH. Each section edits in a CodeMirror 6 markdown editor and saves by name; checkboxes under `## Acceptance criteria` tick through `check_ac`. Every save sends `if_match`; on 409 the page keeps the draft, shows "changed on disk" with Reload and Copy draft. A New button opens a dialog for `POST /api/items`. Unsaved edits warn on navigation. After a save the toast shows the file path and links to History.

Acceptance criteria:
- AC-15.1 Given a changed `priority`, when saved, then the PATCH body holds only `set: {priority}` and `if_match`.
- AC-15.2 Given a section edit, when saved, then the body holds `section: {name, text}`.
- AC-15.3 Given a 409, when received, then the draft text is still in the editor and "changed on disk" shows.
- AC-15.4 Given ticking the second criterion, when clicked, then the body is `{check_ac: 2}`.
- AC-15.5 Given New task, when submitted, then the router opens the created item.
- AC-15.6 Given unsaved edits, when leaving, then a confirm shows.

## Non-functional requirements

- Platforms: Windows, macOS, Linux; Chrome, Firefox and Safari current.
- API read under 100 ms warm; a first page paint under 1.5 s on localhost.
- The UI adds under 5 MB to the release binary; no external requests; no telemetry.
- Rust 1.89; new crates: `axum`, `tokio`, `tower`, `tower-http`, `notify`, `rust-embed` (feature `ui`), `ts-rs` (feature `ts`), `http-body-util` (dev). Web: React, Vite, TanStack Router and Query, Tailwind, shadcn/ui, `@dnd-kit`, `@xyflow/react`, `@dagrejs/dagre`, CodeMirror 6, Mermaid, `github-slugger`, vitest; versions pinned in `pnpm-lock.yaml`.
- Accessibility: all views keyboard reachable; board keyboard drag.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Watcher and lazy refresh racing | Stale or doubled reads | Medium | One refresh function, `BEGIN IMMEDIATE`, event sent after refresh returns, burst test |
| Scope: ten views | Late milestone | High | One agent batch per 2-3 views; each view ships alone behind its route |
| Editing overwrites a concurrent change | Lost work | Medium | `if_match` etag, 409 with draft kept |
| Local server reachable by a web page | Edits by an attacker | Low | Host and Origin checks, loopback default |
| Type drift between Rust and TS | Runtime errors | Medium | ts-rs export plus CI diff |
| Node tooling breaks the Rust workflow | Slow CI | Low | Separate `web` job, `ui` feature off by default |

## Alternatives considered

- WebSocket instead of SSE: needs more code for one-way pushes; ADR-2 picks SSE.
- Hand-written TS types: drift; ts-rs.
- Full-text editor for the whole file: sections and forms keep diffs minimal and frontmatter valid.

## Open questions

None.

## Decision log

- 2026-10-25 Added `GET /api/config`, `GET /api/arch/graph` and `code/modules` to the roadmap's routes because the board, forms and map need them - claude-code
- 2026-10-25 Port 4747 - claude-code
- 2026-10-25 `if_match` etag on writes - claude-code
- 2026-10-25 Host and Origin checks in the server - claude-code
- 2026-10-25 SSE events come from watched paths, not from the watcher's refresh result - claude-code
- 2026-10-25 List rows carry the etag (also in `list --json`) so the board can send `if_match` - claude-code
- 2026-10-25 `GET /api/search` returns the shared `{query, hits, truncated}` shape - claude-code
- 2026-10-25 Release wiring is `pnpm --dir web build`, then `cargo build --release --features ui` (cargo-dist `features`), since Cargo has no per-profile features - claude-code
- 2026-10-25 Tasks are split to about 300 lines (20 tasks: 5 server, 15 web) - claude-code

## Tasks

### T1 Server skeleton, errors and hardening

Scope: `crates/server` with axum, error mapping, host and origin checks, `stone serve` (FR-1); about 250 lines.
Owned files: `crates/server/Cargo.toml`, `crates/server/src/lib.rs`, `crates/server/src/error.rs`, `crates/server/src/guard.rs`, `crates/server/tests/guard.rs`, `crates/engine/src/error.rs`, `crates/cli/src/main.rs`, `Cargo.toml`.
Interfaces: `pub fn router(repo: Repo) -> axum::Router`; `pub async fn serve(repo: Repo, opts: ServeOpts) -> Result<(), ServerError>`; `ServeOpts { host: String, port: u16, watch: bool, open: bool }`; `Error::kind(&self) -> ErrorKind` (new: `NotFound | Invalid | Conflict | BadRequest | Forbidden | Internal`); `ApiError { error: String, kind: String }`; CLI `stone serve` (new).
Tests: `port_taken_message` (AC-1.1), `foreign_host_rejected` (AC-1.2), `not_found_shape` (AC-1.4).
Finish line: `cargo test -p carvestone-server guard` passes.
Depends on: M-4 done.
Risk: normal.

### T2 Read routes and API config

Scope: read routes, `/api/config`, `/api/arch/graph`, list etag (FR-2); about 350 lines.
Owned files: `crates/server/src/routes.rs`, `crates/server/tests/read_routes.rs`, `crates/engine/src/api_config.rs`, `crates/engine/src/arch_graph.rs`, `crates/engine/src/query.rs`, `crates/engine/src/lib.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn api_config(repo: &Repo) -> Result<ApiConfig, Error>`; `ApiConfig { task_prefix: String, statuses: BTreeMap<String, Vec<String>>, priorities: Vec<String>, fields: BTreeMap<String, Vec<FieldSpec>> }`; `FieldSpec { key: String, kind: String, values: Option<Vec<String>> }`; `pub fn arch_graph(repo: &Repo) -> Result<ArchGraph, Error>`; `ArchGraph { nodes, uses, code_edges, drift }`; `ListRow.etag: String` and `GetResult.etag: String` (fields added); `pub fn etag_of(text: &str) -> String` (blake3 hex).
Tests: `items_equal_cli_list` (AC-2.1), `section_get_has_etag` (AC-2.2), `search_equals_cli` (AC-2.3), `ambiguous_code_target_422` (AC-2.4), `config_lists_statuses` (AC-2.5), `row_etag_equals_get_etag` (AC-2.6).
Finish line: `cargo test -p carvestone-server read_routes` passes.
Depends on: T1.
Risk: normal.

### T3 Write routes

Scope: PATCH and POST through the engine (FR-3, AC-1.3); about 250 lines.
Owned files: `crates/server/src/write.rs`, `crates/server/src/routes.rs`, `crates/server/tests/write_routes.rs`, `crates/engine/src/edit.rs`.
Interfaces: `PatchBody { set, unset, section, check_ac, dry_run, if_match }`; `PatchReply { id, changed, etag, text }`; `Repo::edit_if_match(&self, id: &str, edit: Edit, etag: Option<&str>) -> Result<Written, Error>` (new, compares inside the lock).
Tests: `patch_bytes_equal_cli` (AC-3.1), `patch_twice_unchanged` (AC-3.2), `stale_etag_409` (AC-3.3), `done_without_outcome_422` (AC-3.4), `post_creates_task` (AC-3.5), `foreign_origin_403` (AC-1.3). Invariants: the etag is compared after the lock is taken; repeating a write changes nothing.
Finish line: `cargo test -p carvestone-server write_routes` passes.
Depends on: T2.
Risk: normal.

### T4 Watcher and SSE

Scope: `notify` debounce, path-derived events, refresh, `/api/events` (FR-4); about 300 lines.
Owned files: `crates/server/src/watch.rs`, `crates/server/src/events.rs`, `crates/engine/src/ids.rs`, `crates/engine/src/lib.rs`, `crates/server/tests/events.rs`.
Interfaces: `pub fn spawn_watcher(repo: Repo, tx: broadcast::Sender<Event>) -> Result<WatchHandle, ServerError>`; `Event::{Items { ids: Vec<String> }, Code { paths: Vec<String> }, Error { message: String }}`; `pub fn ids_for_paths(repo: &Repo, paths: &[PathBuf]) -> Vec<String>`; route `GET /api/events` (text/event-stream).
Tests: `items_event_on_file_edit` (AC-4.1), `code_event_and_find` (AC-4.2), `patch_emits_event` (AC-4.3), `no_watch_still_reads_fresh` (AC-4.4), `burst_coalesces` (AC-4.5), `event_ids_survive_foreign_refresh` (AC-4.6), `deleted_item_id_in_event` (AC-4.7). Invariants: the event is sent after refresh returns; a dropped client does not affect others.
Finish line: `cargo test -p carvestone-server events` passes.
Depends on: T3.
Risk: high (races, platform watcher differences).

### T5 Embedded UI and type export

Scope: `ui` feature, placeholder, SPA fallback, ts-rs export test, CI diff step (FR-5); about 250 lines.
Owned files: `crates/server/src/assets.rs`, `crates/server/build.rs`, `crates/server/tests/assets.rs`, `crates/engine/tests/ts_export.rs`, `crates/engine/Cargo.toml`, `crates/engine/src/{query,search,code,code_graph,why,history,timeline,changed_files,stale,arch_graph,api_config}.rs` (add `#[cfg_attr(feature = "ts", derive(ts_rs::TS))]` to every API struct), `.github/workflows/ci.yml`.
Interfaces: feature `ui` (server), feature `ts` (engine); `fn asset_response(path: &str) -> Response`.
Tests: `placeholder_without_ui` (AC-5.1), `spa_fallback_with_ui` (AC-5.2, built with the feature on a temp `dist`), `unknown_api_is_json_404` (AC-5.4), `ts_export_writes_bindings` (AC-5.3, `--features ts`).
Finish line: `cargo test -p carvestone-server assets` passes; `cargo test -p carvestone-engine --features ts ts_export` leaves `git diff web/src/api/types` empty.
Depends on: T2.
Risk: normal.

### T6 Web scaffold, CI job and release wiring

Scope: `web/` project and tooling, CI `web` job, release wiring, `web` arch component (FR-6 build side); about 300 lines.
Owned files: `web/package.json`, `web/vite.config.ts`, `web/tsconfig.json`, `web/eslint.config.js`, `web/index.html`, `web/scripts/check-size.mjs`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `dist-workspace.toml`, `stone/arch/carvestone/web.md`, `CLAUDE.md`.
Interfaces: scripts `typecheck`, `lint`, `test`, `build`, `check-size`; CI job `web`; release steps `pnpm --dir web install --frozen-lockfile`, `pnpm --dir web build`, then cargo-dist builds with `features = ["ui"]` (no profile-level feature); the CLAUDE.md check list gains the web commands.
Tests: `check-size` fails when the initial JS exceeds 400 KB gzipped (AC-6.3); a smoke vitest `app_renders`.
Finish line: `pnpm --dir web typecheck`, `lint`, `test` and `build` pass; `cargo run -q --bin stone -- check` exits 0 with the new arch element.
Depends on: T5.
Risk: normal.

### T7 App shell, API client and event hook

Scope: layout, theme, router, API client, SSE hook (FR-6 runtime side); about 300 lines.
Owned files: `web/src/main.tsx`, `web/src/routes/__root.tsx`, `web/src/api/client.ts`, `web/src/api/events.ts`, `web/src/components/ui/**`, `web/src/shell/**`, `web/src/api/events.test.ts`.
Interfaces: `api.get<T>(path, query?)`, `api.patch<T>(path, body)`, `api.post<T>(path, body)` (throw `ApiError { error, kind, status }`); `useEvents()` hook; `<Shell />`.
Tests: `items_event_invalidates_queries` (AC-6.1), `stream_drop_shows_offline_and_retries` (AC-6.2).
Finish line: `pnpm --dir web test -- events` passes.
Depends on: T6.
Risk: normal.

### T8 Markdown rendering

Scope: anchors, wikilinks, Mermaid, code chips (FR-7 rendering); about 300 lines.
Owned files: `web/src/lib/slug.ts`, `web/src/lib/markdown.tsx`, `web/src/components/Mermaid.tsx`, `web/src/lib/slug.test.ts`, `web/src/lib/markdown.test.tsx`.
Interfaces: `slugs(headings: string[]): string[]`; `<Markdown source onLink />`; `<Mermaid chart />` (lazy).
Tests: `anchors_match_engine` (AC-7.1), `wikilink_targets_router` (AC-7.2), `mermaid_error_shown` (AC-7.3).
Finish line: `pnpm --dir web test -- markdown` passes.
Depends on: T7.
Risk: normal.

### T9 Docs site

Scope: sidebar tree, doc route, TOC, backlinks, stale banner (FR-7 site); about 300 lines.
Owned files: `web/src/routes/docs.$.tsx`, `web/src/features/docs/**`, `web/src/features/docs/docs.test.tsx`.
Interfaces: `<DocsPage id />`; `<Sidebar tree />`; `<Backlinks items />`; `<Toc sections />`; `buildTree(rows: ListRow[]): TreeNode[]`.
Tests: `backlinks_listed` (AC-7.4), `tree_orders_by_order_then_title`.
Finish line: `pnpm --dir web test -- docs` passes; user click-list: open `/docs/prd`, expand the sidebar, follow a `[[...]]` link, see the Mermaid diagram.
Depends on: T8.
Risk: normal.

### T10 Item page core

Scope: header, trust badge, fields, sections, links, short-id redirect, not-found page (FR-8 core); about 300 lines.
Owned files: `web/src/routes/items.$.tsx`, `web/src/features/item/ItemPage.tsx`, `web/src/features/item/TrustBadge.tsx`, `web/src/features/item/item.test.tsx`.
Interfaces: `<ItemPage id />`; `<TrustBadge trust />`.
Tests: `short_id_redirects` (AC-8.2), `not_found_shows_suggestion` (AC-8.3), `trust_badge_from_engine_value`.
Finish line: `pnpm --dir web test -- item` passes.
Depends on: T7.
Risk: normal.

### T11 Item history, provenance and extras

Scope: History and Provenance tabs, changed files, code dependencies, libraries, stale banner (FR-8 rest); about 300 lines.
Owned files: `web/src/features/item/HistoryTab.tsx`, `web/src/features/item/ProvenanceTab.tsx`, `web/src/features/item/Extras.tsx`, `web/src/features/item/history.test.tsx`.
Interfaces: `<HistoryTab id anchor? />`; `<ProvenanceTab item />`; `<ChangedFiles files />`.
Tests: `provenance_lists_all_verified` (AC-8.1), `history_entries_render` (AC-8.4).
Finish line: `pnpm --dir web test -- history` passes; user click-list: open a task, a decision and a component and check the tabs.
Depends on: T10.
Risk: normal.

### T12 Board

Scope: FR-9; about 350 lines.
Owned files: `web/src/routes/board.tsx`, `web/src/features/board/**`, `web/src/features/board/board.test.tsx`.
Interfaces: `<Board />`; `useMoveTask()` (optimistic mutation with rollback; sends the row's etag as `if_match`).
Tests: `drop_sends_patch_with_etag` (AC-9.1), `error_rolls_back_with_toast` (AC-9.2), `milestone_filter_from_url` (AC-9.3), `conflict_refetches` (AC-9.4).
Finish line: `pnpm --dir web test -- board` passes; user click-list: drag a card to in-progress, then to done without an Outcome, then check `git diff`.
Depends on: T7.
Risk: normal.

### T13 Milestones and plans

Scope: FR-10; about 250 lines.
Owned files: `web/src/routes/milestones.tsx`, `web/src/routes/plans.tsx`, `web/src/features/planning/**`, `web/src/features/planning/planning.test.tsx`.
Interfaces: `<MilestoneList />`, `<MilestonePage id />`, `<PlanList />`, `<PlanPage id />`.
Tests: `milestone_progress_3_of_4` (AC-10.1), `plan_lists_tasks` (AC-10.2).
Finish line: `pnpm --dir web test -- planning` passes.
Depends on: T10.
Risk: normal.

### T14 Search

Scope: FR-11; about 250 lines.
Owned files: `web/src/routes/search.tsx`, `web/src/features/search/**`, `web/src/features/search/search.test.tsx`.
Interfaces: `<CommandPalette />`; `useSearch(query, filters)` (debounced 200 ms).
Tests: `debounce_one_request` (AC-11.1), `hit_opens_section` (AC-11.2), `symbol_hit_opens_explorer` (AC-11.3).
Finish line: `pnpm --dir web test -- search` passes; user click-list: press Ctrl+K, search a phrase, open a hit.
Depends on: T9.
Risk: normal.

### T15 Architecture map

Scope: FR-12; about 350 lines.
Owned files: `web/src/routes/architecture.tsx`, `web/src/features/arch/**`, `web/src/features/arch/arch.test.tsx`.
Interfaces: `<ArchMap />`; `layout(graph: ArchGraph): { nodes: Node[]; edges: Edge[] }` (dagre).
Tests: `drift_edge_red_with_panel` (AC-12.1), `draft_dashed` (AC-12.2), `hide_code_edges` (AC-12.3).
Finish line: `pnpm --dir web test -- arch` passes; user click-list: open `/architecture` on this repo and click a component.
Depends on: T11.
Risk: normal.

### T16 Code explorer: search, detail and tabs

Scope: symbol search, detail pane, Uses, Dependencies, Impact, Modules, Path form (FR-13 lists); about 300 lines.
Owned files: `web/src/routes/code.tsx`, `web/src/features/code/Explorer.tsx`, `web/src/features/code/tabs/**`, `web/src/features/code/explorer.test.tsx`.
Interfaces: `<Explorer />`; `useCode(op, args)`.
Tests: `ambiguous_lists_candidates` (AC-13.1), `impact_headings` (AC-13.3), `no_path_message` (AC-13.4).
Finish line: `pnpm --dir web test -- explorer` passes; user click-list: search a symbol, open Uses and Impact.
Depends on: T11.
Risk: normal.

### T17 Code explorer: neighbourhood graph

Scope: React Flow neighbourhood with exact and inferred edges (FR-13 graph); about 250 lines.
Owned files: `web/src/features/code/Neighbourhood.tsx`, `web/src/features/code/neighbourhood.ts`, `web/src/features/code/neighbourhood.test.tsx`.
Interfaces: `neighbourhood(hits: Hit[], depth: number): { nodes: Node[]; edges: Edge[] }`; `<Neighbourhood symbol depth hideInferred />`.
Tests: `hide_inferred` (AC-13.2).
Finish line: `pnpm --dir web test -- neighbourhood` passes; user click-list: toggle inferred edges and depth.
Depends on: T16.
Risk: normal.

### T18 Timeline

Scope: FR-14; about 200 lines.
Owned files: `web/src/routes/timeline.tsx`, `web/src/features/timeline/**`, `web/src/features/timeline/timeline.test.tsx`.
Interfaces: `<Timeline />`; `useTimeline(filters, limit)`.
Tests: `kind_chip_sets_param` (AC-14.1), `load_more_doubles_limit` (AC-14.2).
Finish line: `pnpm --dir web test -- timeline` passes.
Depends on: T10.
Risk: normal.

### T19 Editing: field forms

Scope: Edit mode, field forms from specs, `if_match` handling, `check_ac`, unsaved warning (FR-15 fields); about 300 lines.
Owned files: `web/src/features/edit/FieldForm.tsx`, `web/src/features/edit/useSaveItem.ts`, `web/src/features/edit/fields.test.tsx`, `web/src/features/item/ItemPage.tsx`.
Interfaces: `<FieldForm item spec />`; `useSaveItem(id)` sends `if_match`; `FieldSpec { key, kind, values? }` from `/api/config`.
Tests: `only_changed_keys_sent` (AC-15.1), `check_ac_body` (AC-15.4), `leave_warns` (AC-15.6).
Finish line: `pnpm --dir web test -- fields` passes; user click-list: edit a priority, tick a criterion, run `git diff`.
Depends on: T3, T10.
Risk: normal.

### T20 Editing: sections and New item

Scope: CodeMirror section editor, 409 draft handling, New dialog (FR-15 sections); about 300 lines.
Owned files: `web/src/features/edit/SectionEditor.tsx`, `web/src/features/edit/NewItemDialog.tsx`, `web/src/features/edit/sections.test.tsx`.
Interfaces: `<SectionEditor name text onSave />` (CodeMirror 6); `<NewItemDialog />`.
Tests: `section_save_body` (AC-15.2), `conflict_keeps_draft` (AC-15.3), `new_task_opens_item` (AC-15.5).
Finish line: `pnpm --dir web test -- sections` passes; user click-list: edit a section, create a task, check `git diff`.
Depends on: T19.
Risk: high (conflicts, editor integration).
