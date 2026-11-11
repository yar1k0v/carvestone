---
type: plan
title: M-9 multi-repo product workspace requirements
status: done
created: 2026-10-26
generated: { by: claude-code, at: 2026-11-07 }
---

## Summary

One stone for a whole product, for any product: a root folder that holds `stone/`, any number of project folders, and any number of plain doc folders. Each of them may be its own git repo, or the root may be one repo for all. Nothing is fixed to one layout; `stone/config.yml` says where things are, and git repos are found by themselves.

Example (Freightline, one of many):

```text
Freightline/                 plain folder, no git; `stone` runs here
  .stone/index.db       one index, not in any repo
  stone/                git repo: tasks, docs, decisions, architecture
  docs/                 the user's notes, indexed for search, never written
  api/  portal/  ...    project repos, each with its own git
```

```yaml
# stone/config.yml
project: freightline
task_prefix: VC
code_roots: [api/, portal/, notifier/]   # any folders, one repo each or shared
doc_roots: [docs/]                       # optional; read-only markdown for search
```

The same config works for carvestone today (`code_roots: [crates/, web/]`, one repo at the root) and for a product with ten repos. Paths stay relative to the root (`api/src/Orders.cs`, `stone/tasks/VC-1.md`), so code globs, ownership, drift and links work unchanged. What changes is git: each path belongs to the nearest git repo that holds it, and every git call goes to that repo with the path translated. One code path covers every layout.

## Requirements

- FR-1 Workspace root. `stone init` works in a folder without git. `.stone/` is never inside a repo that would track it; when the root is not a repo, nothing is written to a `.gitignore`. Discovery from any subfolder (a project, `stone/`, `docs/`) finds the product root.
  - AC-1.1 Given a plain folder, when `stone init --prefix VC` runs, then `stone/config.yml` exists and no `.gitignore` is written outside `stone/`.
  - AC-1.2 Given the cwd is `api/src`, when `stone list` runs, then it reads the product store, not a store in `api/`.
- FR-2 Git per path. A resolver maps every root-relative path to its repo (nearest `.git` at or below the root) and back. Every git call goes through it: commit cache, blame, `why`, `history`, `timeline`, a task's changed files, staleness, id branch checks, author, `ls-files` for the code index and ownership, and the commit-msg hook.
  - AC-2.1 Given commits with `Refs: VC-3` in `api/` and in `portal/`, when `stone get VC-3` runs, then Changed files lists both, with root-relative paths.
  - AC-2.2 Given `why api/src/Orders.cs:12`, then blame comes from the `api` repo and the line's task is found.
  - AC-2.3 Given `history VC-3`, then the diffs come from the `stone` repo.
  - AC-2.4 Given ids `VC-1..VC-4` on another branch of the `stone` repo, when `stone new task` runs, then it gets `VC-5`.
  - AC-2.5 Given a path in no repo, git-backed answers leave it out with no error; `why` says the file is not under git.
  - AC-2.6 A one-repo setup gives the same output as before M-9 (existing tests unchanged).
- FR-3 Code per project. `code_roots` lists the project folders. Each project's files come from its own repo's `ls-files` (untracked included, ignores honoured). The code graph, Louvain modules and `scan` respect project boundaries.
  - AC-3.1 Given `code_roots: [api/, portal/]`, then `stone check` requires every tracked file in both to have exactly one owner.
  - AC-3.2 `stone code uses` finds a caller in the same project; cross-repo edges appear only where a manifest reference resolves.
- FR-4 Plain doc folders. New config `doc_roots` (for example `[docs/]`). Their markdown is indexed as read-only sections for search and links; stone never writes there and `check` does not demand frontmatter.
  - AC-4.1 Given `docs/setup.md` with a heading "Deploy", when `stone search deploy` runs, then the hit is `docs/setup.md#deploy`.
  - AC-4.2 `stone edit` on a doc_roots file fails with a sentence saying the folder is read-only.
- FR-5 Product architecture. One system item for the product, one component (or container) per project with `code: ['api/**']`, sub-components below it. Drift between projects uses the existing component edges. `scan` drafts one component per project when none owns it. The web architecture map opens at the product level and drills into a project.
  - AC-5.1 Given `portal` code calling `api` code with no `uses: [arch/freightline/api]`, then `stone check` warns arch-drift.
  - AC-5.2 The map shows the system with its projects and their links; clicking a project shows its components.
- FR-6 Watcher and server. `stone serve` at the product root watches `stone/`, every code root and every doc root, and refreshes the right index part.
  - AC-6.1 Editing `docs/setup.md` pushes an SSE event and the search hit changes.
- FR-7 Switch-over. `docs/switch-over` gets a product-workspace section: init at the root, `stone/` as its own repo, `code_roots` and `doc_roots`, importers run per old tool folder, hooks installed in every project repo.

## Tasks

In order; each is one task.

1. Repo resolver in the engine (FR-2 core): discover repos under the root, map paths both ways, one `Git` per repo. A fixture helper that builds a product folder with three repos. High: every git call site moves to it.
2. Workspace init and discovery (FR-1).
3. Git call sites through the resolver (FR-2): commit cache keyed by repo and sha (index schema bump), blame, why, history, timeline, changed files, staleness, ids, author, commit-msg hook per repo.
4. Code index and ownership per repo (FR-3): `ls-files` per project repo, project boundaries in modules and scan, native SCIP paths per project.
5. `doc_roots` (FR-4): config field, sections from plain markdown, read-only guard, check rules.
6. Product architecture (FR-5): scan per project, drift across projects, web map with product level and drill-down.
7. Watcher and server over all roots (FR-6).
8. Switch-over doc and an end-to-end test on the three-repo fixture (FR-7).

Risks: the commit cache schema change (rebuild only, no migration, per ADR-1); git speed with many repos on Windows (one cache refresh per repo, run only for repos whose HEAD moved); a project that is itself the root repo (allowed, it's the one-repo case).

Open: whether cross-project `uses` should also come from runtime links (HTTP, queues) declared in `## Uses` bullets; they already work for components today, so nothing new is needed unless the map should draw them differently.
