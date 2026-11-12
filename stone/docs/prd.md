---
type: doc
title: PRD
description: 'Historic requirements from October 2026. Current docs: docs/product-overview.'
status: deprecated
generated: { by: claude-code, at: 2026-11-11 }
verified: [{ by: claude-code, at: '2026-11-12T16:48:12+00:00' }]
---

# Carvestone — PRD

Oct 23, 2026 · @Yaroslav · replaces the Oct 16 context-bundle PRD

## Why

Today a project runs on separate tools that don't know about each other:

| Need | Tool today |
| --- | --- |
| Tasks, board, milestones, decisions | Backlog.md |
| Architecture map | Groma.md |
| Code dependencies, modules, impact | graphify (or another code graph) |
| Project docs as a site | fumadocs |
| Third-party library docs | Docs MCP |

Each one has its own folder, index, CLI and MCP, and none can link to the others. An agent has to learn five interfaces and stitch the answers together itself. A task can't point at a code symbol, a doc can't show which component it describes, and nothing says which commit did which task.

Carvestone is one binary, CLI `stone`, that holds four linked parts in one store, one index, one link graph, one CLI and one MCP:

- **Planner:** tasks, a board, milestones, plans and decisions.
- **Docs:** project docs, readable as a site.
- **Architecture:** systems, containers and components that own code.
- **Code graph:** symbols, dependencies, modules, impact and paths.

Git links join the parts: every commit is tied to the task, plan and decision behind it (ADR-16).

Library docs stay in Docs MCP; Carvestone links to them.

The questions it answers in one call, for an agent or a person:
- Where is this documented?
- What is this code, what uses it, and what breaks if I change it?
- What is planned, and what's next?
- What was done, when, and why?

The name: a carved stone is a lasting record one party leaves for the next to read. The data folder is `stone/`.

## Goals

1. **Four parts, linked.** Planner, docs, architecture and code graph share one store, one index and one link graph. The claim is the links, not the feature count.
2. **Everything linked.** Task ↔ plan ↔ milestone ↔ decision ↔ doc ↔ component ↔ code symbol ↔ commit. Backlinks come free.
3. **Cheap, accurate, fast answers.** Section-level search and graph queries return small results with ids and line ranges, never whole files. The agent expands only what it needs. Accuracy and speed are measured in CI.
4. **Code structure for agents.** Symbols, imports and calls, module grouping, impact of a change, and the path between two parts of the code. Kept fresh on every file change.
5. **Git tied to the plan.** From any file, task or plan you can see what changed, when and why.
6. **Agents first, humans second.** CLI with `--json` and an MCP server for agents; a local web UI for people.
7. **Single static binary.** Offline, no account; the web UI is embedded.
8. **Open format.** Curated knowledge is plain markdown in git, as an OKF v0.2 profile.

Non-goals for now:
- Library docs. Docs MCP keeps doing that; we link to it.
- A file store for images, decks and PDFs (after the MVP; see Files).
- A hosted multi-user server. Local first; teams share through git.
- Replacing git hosting, CI or Jira.

## Principles

| Principle | In practice |
| --- | --- |
| Agents first, humans second | Formats and outputs are built for an agent that reads in few tokens and writes through the CLI. The web UI makes the same data comfortable for people |
| Curated knowledge in git, derived data in the index | Tasks, docs, decisions and architecture are files in git. The code graph, search index and git cache are derived, live in `.stone/` and can be rebuilt at any time |
| Minimal diffs | Writes patch only the changed frontmatter lines |
| OKF-compatible | Every item is an OKF concept with OKF field names where OKF has them; unknown keys are kept |
| Stable ids | Files are named by id, never by title, and never move |
| Links over guessing | Explicit links and graph edges rank above text matches |
| One engine, many faces | CLI, MCP and web UI all call the same engine |

## Data model

Each item is one markdown file with frontmatter. Its OKF concept id is its path without `.md`; short ids (`CS-12`, `ADR-3`) resolve to it.

| Type | Path | Key fields |
| --- | --- | --- |
| `doc` | `stone/docs/**` (folders = site nav) | `title`, `description`, `order`, `components` |
| `plan` | `stone/plans/<slug>.md` | `status`, `milestone`, links to its tasks |
| `task` | `stone/tasks/CS-12.md` | `status`, `priority`, `kind`, `milestone`, `plan`, `depends_on`, `assignee`, `components`, `paths`; sections `## Description`, `## Acceptance criteria`, `## Plan`, `## Notes`, `## Outcome` |
| `milestone` | `stone/milestones/M-3.md` | `status`, `target`, goal |
| `decision` | `stone/decisions/ADR-7.md` | `status`, `supersedes`; context, decision, consequences |
| `reference` | `stone/refs/<slug>.md` | `resource` (URL), `library`, `version`, why it matters; library refs point at Docs MCP |
| `system` / `container` / `component` | `stone/arch/**` | `code` globs, `entry`, `technology`, `uses`, `libraries`; see Architecture |

```yaml
type: task
title: Date-range pull for RO gate events
description: Paged partner endpoint for Northgate, last 4 months.
status: in-progress
links: [docs/integrations/ro-gate-events, ADR-7]
components: [arch/freightline/api/integrations]
paths: [freightline/Freightline.Domain/Features/Consignments/GetRoGateEventsQuery.cs]
generated: { by: 'claude-code', at: 2026-10-15 }
verified: { by: 'human:yaroslav', at: 2026-10-16 }
```

Links:
1. Typed relations in frontmatter: `depends_on`, `milestone`, `plan`, `components`, `supersedes`, `parent`, `uses`, plus untyped `links`.
2. Body links: markdown links, `[[CS-12]]`, `[[docs/a#section]]`, and code links `[[code:Freightline.Domain.GetRoGateEventsQuery]]` or `[[code:path/file.cs:120-160]]`.
3. Code ownership: `paths` and component `code` globs.
4. Commits: trailers `Refs: CS-12`, `Plan: plans/x`, `Decision: ADR-7`.

The index computes backlinks for all of these, code links included. A symbol knows the docs and tasks that mention it.

Task rules, kept from Backlog.md where they work:
- The definition of done lives in config.
- One task fits one agent context and one PR.
- Files never move; archive is a status.
- New ids come from the highest id across branches plus one, under a lock; `check` and `renumber` handle collisions.
- A task's changed files come from git (trailer commits plus the working diff), not from lists kept by hand.

## Search

Search is the agent's main entry point, so it has its own targets.

- **Units:** sections by heading for docs, tasks, plans and decisions; architecture elements; code symbols with their signature and doc comment. Each unit keeps its id, anchor and line range.
- **Ranking:** FTS5 BM25 with title and heading weights, plus a boost for anything linked to the context given (`--from CS-12`, `--component X`). Filters for type, status, milestone and component. Embeddings later, only to fill gaps.
- **Output:** about 40 tokens per hit (`docs/integrations/ro-gate-events#auth  L12-40  Basic auth with hardcoded…`), capped by `--budget N`. Expand with `get <id>#section`. `--json` has the same fields.
- **Speed:** lazy reindex by content hash; under 50 ms per query on 5,000 sections.
- **Accuracy:** a fixed Freightline query set with expected hits; recall@5 ≥ 0.9, with tokens per answered question tracked. It runs in CI.

## Code graph

Takes graphify's job: code structure an agent can query cheaply.

- **Parse:** 5 built-in languages (C#, Python, TypeScript/JavaScript, Rust, Dart), plus any SCIP index listed in `stone/config.yml`. For a language with no parser, SCIP creates the files and symbols (ADR-17). Config holds data only: the user or CI runs the indexer.
- **Nodes:** project (csproj, pyproject, package.json, Cargo.toml), module or namespace, file, type, function or method. Each node has its signature, doc comment and line range.
- **Edges:** contains, imports, calls, inherits/implements, references.
  - An edge is `exact` when it resolves through imports and scope, and `inferred` when it matches by name only.
  - Outputs show the difference, so an agent knows what to double-check.
- **Modules:** community detection (Louvain) over the edges, within project and folder boundaries, groups files into modules. New modules come up as draft components in `stone/arch/`.
- **Storage:** derived, in the SQLite index under `.stone/`, never in git. Rebuilt incrementally from changed files: the watcher in `serve`, lazily on every CLI call.
- **Queries** (all compact, ids and line ranges, `--budget`):

| Command | Answers |
| --- | --- |
| `stone code find <name>` | Where a symbol is defined |
| `stone code outline <file\|symbol>` | What a file declares, nested and in line order; for a type, it and its members |
| `stone code uses <symbol\|file>` | Who calls, imports or inherits it |
| `stone code deps <symbol\|file>` | What it depends on |
| `stone code impact <symbol\|file>` | What may break: transitive dependents, their components, open tasks and docs on them |
| `stone code path <a> <b>` | How two parts connect |
| `stone code explain <module\|component\|folder>` | Entry points first, then for a component or folder its top folders with the code edges between them, the packages its project files name and the outside projects they reference (libraries, each a folder to explain in turn); then what the component does with each component it uses, files, key symbols, inbound and outbound dependencies, docs, tasks |
| `stone code entries [<component>]` | Where execution starts: declared and detected entry points per component |
| `stone code native [--index <file.scip>]...` | Exact edges from the language SDKs' indexers (see below) |

- **Native pass:** tree-sitter stays the fast, always-on index. `stone code native` adds the edges the SDKs resolve through types and generics.
  - With no `--index`, it runs the indexers it finds on `PATH` for the languages the repo holds: `rust-analyzer scip`, `scip-typescript index`, `scip-dotnet index`, `scip-python index`, `scip_dart`. It reports which ran and which it skipped, and never installs anything; a skipped indexer names its install command. With `--index`, it reads the given SCIP files instead.
  - scip-python as released stops at start-up on Windows, and its pip step fails there. On Windows stone starts its script under node with a preload that fixes the start-up and hands it an empty package list; links into the repo's own code need no packages.
  - Dart and Flutter packages use `scip_dart` (`dart pub global activate scip_dart`, with the pub cache's `bin` folder on `PATH`). It runs in each folder with a `pubspec.yaml` and needs `dart pub get` (or `flutter pub get`) there first; stone does not run it, and a package without its package config fails with that step named.
  - A symbol two projects define under one name (two `Auth.ITokenManager` in two services) joins the declaration in the same index as the reference; another project's only when its own declares none.
  - Progress: on a terminal each indexer's line prints as it ends, with one live line for the running one (repo, run count, seconds, the tool's last output line). The Code page runs the same pass from a button and shows the same steps; one pass runs at a time. A failed indexer reports its own error line, not a runtime trailer.
  - A native edge replaces the tree-sitter edges at the same site (same source symbol, line and name): a name-only guess, a bucket or a dispatch edge becomes one exact edge. A site resolved outside the repo (the standard library, dependencies) loses its guesses and gains no edge. Queries show native edges as `exact`.
  - A guess whose site and target both sit in files the indexer covered is dropped: it saw both ends and did not link them. A guess into a file no indexer covered stays, and so does a `dispatch` guess, since not every indexer reports implementations.
  - A call in a multi-line chain is recorded on the line of its name, as the indexers report it.
  - SCIP positions are columns, ours are lines, so a site is matched by file, line and the identifier text its range covers; SCIP symbols are joined to our declarations through their definition occurrences. An `is_implementation` relationship between two members becomes a `dispatch` edge. See `crates/engine/src/code_native.rs` for what does not map one to one.
  - Native edges live in their own tables. A tree-sitter refresh never rewrites them; a replaced file takes the native edges that start or end in it, and code queries then say `native edges are out of date for N files (<project>); run stone code native`. Without a native run nothing changes, and a second run stores the same edges.

- **Targets:** a full Freightline index in under 60 s and under 1 s per changed file. `uses` finds at least 90% of the callers an IDE's "find usages" finds, on a fixed sample.

## Architecture

C4 elements are curated items in `stone/arch/`, one file each, nested by folder. The format is our own and built for agents: a short frontmatter plus fixed sections, readable in one call.

```yaml
type: component
title: Partner integrations
description: Basic-auth partner endpoints (Northgate RO gate events, consignments).
technology: ASP.NET Core
code: [freightline/Freightline.Api/Controllers/IntegrationsController.cs, freightline/Freightline.Domain/Features/Consignments/GetRo*.cs]
entry: [freightline/Freightline.Api/Controllers/IntegrationsController.cs#Freightline.Api.Controllers.IntegrationsController]
uses: [arch/freightline/timescale, arch/freightline/postgres]
libraries: [mediatr@12, fluentvalidation@11]
status: stable
```

- **Sections:** `## Responsibilities`, `## Uses`, `## Interfaces` (endpoints, topics, tables), `## Gotchas`. Other sections are kept as written.
- **Ownership:** `code` globs make every file resolve to exactly one element. A file under the globs of an element and of one below it (by `parent`) belongs to the one below, so a container can claim its folder inside its system's. `stone check` reports files owned twice, or by no one within `code_roots`.
- **Service folders:** `stone map` lists each container and component without `code` with the project folder named like it (a `.csproj`, `package.json`, `pyproject.toml` or `Cargo.toml`, by project or folder name). `--write` saves the single matches as `code: [<folder>/**]`; `--pick <id>=<folder>` saves one by hand. A service that names its code is never changed.
  - It also lists the shared projects the services reference, directly or through each other, that no element names: each becomes a component beside its first service, under the parent most of its services share, owning its folder. Test and benchmark projects are never proposed. `--write` adds them.
- **Not architecture:** tests, benchmarks and generated code (EF migrations, designer files, `*.g.cs`, `*.generated.*`) count for no element's dependencies: `arch-drift` and the inbound and outbound lists of `stone code explain` leave their edges out.
- **Declared by the build:** a project reference is a dependency written down already. An element whose project references, directly or through other projects, a project another element owns gets no `arch-drift` warning for using it; `uses` need not repeat it. Code that depends on another element with no project reference behind it still warns.
- **Entry points:** `entry` lists where execution of the element starts, each as `path#Symbol` or `path`. It is optional, and curated like `code`.
  - `stone check` warns when a listed file or symbol is no longer in the code graph, and says to fix the path or run `stone code entries <id>`.
  - `stone code explain <id>` prints the declared entries first, then the detected ones in the element's files. `stone get` shows the `entry` field as written.
  - Detection is deterministic, from the tree-sitter graph and a few source lines:
    - Rust: a top-level `fn main`.
    - C#: a `static Main`; top-level statements.
    - TypeScript and JavaScript: the files a `package.json` `bin` or `main` names (a build folder such as `dist/` maps to `src/`); a file that calls `listen` or `createServer` at file level.
    - Python: a file with an `if __name__ == "__main__":` guard.
  - `stone scan` drafts list the detected entries of their files in `entry`.
- **Described links:** an optional `## Uses` section says what the element does with each element in `uses`, one bullet per target: `- [[arch/freightline/timescale]]: Stores gate events`.
  - The text is a verb phrase of two to five words; `stone check` warns about a bullet whose target `uses` does not list, and about a description of more than eight words. Several reasons for one target are joined with `;`, each part within the limit.
  - `stone code explain` and the architecture map's JSON (`description` on a `uses` edge) carry the phrase.
  - A bullet may end with how the link works in parentheses: `- [[arch/freightline/api]]: Reads and edits items (HTTP)`. Only a trailing parenthesised technology counts, and the word check ignores it. When it is not in-process (HTTP, gRPC, a queue, SQL, a file or similar), drift does not warn that no code edge backs the target; a target with no bullet, or with an in-process technology, still warns. `stone arch connect --over HTTP` writes the trailing `(over HTTP)`.
- **Graph-backed:**
  - Dependencies between components come from the code graph and show next to the curated `uses`.
  - Drift is reported when the code depends on a component that `uses` doesn't list, or the other way round.
  - Module clusters seed new draft components.
  - The scanner never edits curated fields or text, and running it twice gives the same files.
- **Libraries:** `libraries` lists what a component uses. `stone instructions` tells agents to look those up in Docs MCP (`search_docs`).
- **C4 diagrams:** stone draws levels 1 to 3 (context, container, component) from the arch items and their connections (ADR-20). Agents edit data, never layout.
  - `kind` is optional: `person` or `external` on a system, `store` on a container.
  - A product with several systems has a parent system. Its `parent` links the others to it, and it is the level 1 box.
  - C4 Dynamic flows stay as mermaid in docs. They describe one use case, not the structure.
  - `stone arch connect` writes a connection, `stone arch gaps` lists what the design lacks, `stone arch c4` prints a diagram, and `stone instructions design` guides agents.

## Git and history

- **Commit links:** trailers tie commits to tasks, plans and decisions. `stone commit-msg` prints the trailer for the active task, and an opt-in hook checks for one.
- **From code to plan:** `stone why <path>[:line]` shows the commits behind the lines, the tasks and plans they reference, the owning component, and its docs and decisions.
- **Versions:** `stone history <id>` shows an item's git history with diffs.
- **When:** `stone timeline [--since] [--component]` lists tasks done, decisions taken, docs changed and commits, newest first.
- **Trust:**
  - `generated` says who wrote an item, and `verified` that someone confirmed it (a human, or an agent that checked it against current code).
  - Items are flagged as possibly stale when the code under their components changed many commits ago, or `stale_after` passed.

## Interfaces

**CLI** (agents and scripts). Every read has `--json`; every write has `--dry-run`.

| Command | Purpose |
| --- | --- |
| `stone init` | Create `stone/` and config |
| `stone instructions` | Short workflow guide for agents |
| `stone new <type>` / `edit` / `get <id>[#section]` / `list` | Items of any type |
| `stone task move <id> <status>` / `next` | Board moves; ready tasks by priority |
| `stone link <id> <id>` | Add a link |
| `stone search <query>` | Docs, tasks, architecture and code symbols, in one search |
| `stone code find / outline / uses / deps / impact / path / explain / entries / native` | Code graph |
| `stone config add-code-root / add-scip-index` | Change `stone/config.yml` without editing the file |
| `stone arch hld / lld / c4 / connect / gaps` | Design views, C4 diagrams, written connections, what the design lacks |
| `stone map` / `stone scan` | Propose service folders; propose drafts from the graph |
| `stone why <path>[:line]` / `history <id>` / `timeline` | Git and history |
| `stone check` / `renumber` | Validation; CI runs `check` |
| `stone mcp` | Optional read-only MCP server (stdio) |
| `stone serve` | Web UI + HTTP API + watcher |

**Agents use the CLI.** `stone instructions` prints the workflow guides: overview, plan a task, do a task, finish a task, debug and design. **MCP** is opt-in, for clients that can't run a shell, and nothing turns it on by default. It is read-only: `search`, `get`, `list` and `code`. Writes always go through the CLI.

**Web UI** (`stone serve`, localhost):
- docs site with a sidebar from the folder tree, Mermaid and backlinks;
- board;
- milestones and plans;
- search;
- item pages with links, history and provenance;
- System and Solution architecture, C4 levels 1 to 3;
- code graph explorer: modules, dependencies, impact;
- timeline;
- owner actions on tasks (accept, send back, change status), where each action is a file write you see in git. Text and field edits stay in the CLI (ADR-14).

## Importers

Cut in ADR-16.

## Files (after the MVP)

When it comes, files attach to items the way PocketBase attaches them to records. Where they live (git folders with LFS, a local database file or a virtual file system) is decided then. Their text (PDF, DOCX, PPTX, XLSX) goes into search.

## Multi-repo workspaces (after the MVP)

Each repo has its own `stone/`. A workspace file lists repos, so search, the code graph and the board span them all. Cross-repo links use a repo prefix (`freightline-notifier:CS-4`).

## Architecture and stack

```text
cli, mcp, server  ->  engine  ->  core, store, index, graph, git
```

| Layer | Tech |
| --- | --- |
| Core | Item model, ids, relations, provenance |
| Store | `serde_norway` reads + own line-patch writer, atomic writes, advisory lock (`File::lock`) |
| Index | `rusqlite` (bundled) with FTS5; links, symbols and edges tables; lazy reindex by content hash |
| Graph | `tree-sitter` + grammars (C#, Python, TS/JS, Rust); own resolver per language; Louvain clustering; queries in SQL plus `petgraph` |
| Git | `git` CLI (`log`, `blame -p`, trailers); cached in the index by commit |
| Server | `axum`, `tokio`, SSE, `notify`, `rust-embed` |
| MCP | `rmcp` |
| UI | React, Vite, TanStack Router + Query, shadcn/ui, Tailwind, `@dnd-kit`, React Flow, CodeMirror 6, Mermaid |
| Shared types | `ts-rs` |
| Release | `cargo-dist`; static builds for Windows, macOS, Linux |
| Toolchain | Rust 1.89, edition 2024 |

## Milestones

1. **M-1 Store and CLI.** All item types, OKF-profile frontmatter, ids, lock, CRUD, links, `check`, CI.
2. **M-2 Search.** Section units, BM25 with link boost, `--budget`, `get #section`, backlinks; query-set benchmark.
3. **M-3 Code graph and architecture.** tree-sitter graph for 4 languages, exact/inferred edges, modules, `code` queries, symbols in search, ownership, graph-backed architecture with drift and drafts.
4. **M-4 Git and history.** Trailers, `why`, `history`, `timeline`, changed files from git, staleness.
5. **M-5 Agents.** MCP server, workflow guides, `instructions` pointing at Docs MCP for libraries.
6. **M-6 Web UI.** Docs site, board, plans, search, item pages, architecture map, graph explorer, editing.
7. **M-7 Importers (cut, ADR-16).** Backlog.md and Groma.md, then switch Freightline over.
8. **Later.** File store, static export, multi-repo workspace, team mode, embeddings.

Dogfood: this repo from M-1. Freightline is the search benchmark in M-2, the code-graph benchmark in M-3, and moves over fully in M-7.

## Risks

| Risk | Mitigation |
| --- | --- |
| Scope: four tools in one | Strict order; each milestone is usable alone; agent value (search, graph) before the UI |
| Name-based call edges are wrong | `exact` vs `inferred` on every edge; IDE "find usages" sample as the bar |
| tree-sitter resolution per language is a lot of work | Four languages first; plugin grammars later; imports before calls |
| Search returns the wrong sections | Query-set benchmark in CI, link boost |
| Commits without trailers | `commit-msg` helper and hook; `why` falls back to blame → component → open tasks |
| Scanner overwrites curated architecture | It writes drafts and its own fields only; idempotency test |
| OKF spec changes | Profile in one place; unknown keys kept |
