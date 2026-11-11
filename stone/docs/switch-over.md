---
type: doc
title: Switch-over checklist
description: The steps to move a repo from Backlog.md, Groma.md, graphify and fumadocs to stone, with a check list at the end and a way back.
status: stable
generated: { by: claude-code, at: 2026-10-27 }
---

# Switch-over checklist

Work through the steps in order, in a branch. Each import is its own commit, so the way back is a revert. Nothing here deletes anything until step 10.

## 1. Install and start the store

Install the `stone` binary from the release and put it on the path. In the repo root, create the store with the task prefix the old tracker used:

```
stone init --prefix VC
```

Use your own prefix in place of `VC`. Commit the new `stone/` folder.

## 2. Import the Backlog.md folder

Look first. A dry run prints every target with `new`, `changed`, `unchanged` or `skipped`, then the counts, and writes nothing:

```
stone import backlog backlog --dry-run
```

An unmappable status stops the whole import with the value, the file and the allowed list; fix the source (or the status vocabulary in `stone/config.yml`) and run the dry run again. Then import for real and commit:

```
stone import backlog backlog
```

Running it again is safe: unchanged sources change nothing, and a task you edited in stone since is skipped with a message. The import never deletes a file; a task whose source is gone is listed as `source gone: <path>`.

## 3. Import the Groma.md folder

```
stone import groma groma --dry-run
stone import groma groma
```

Elements become files under `stone/arch/`, flows under `stone/docs/flows/`. A relationship target that cannot be found stops the import with the file and line; fix it in the source and run again. Commit.

## 4. Check the store and fix what it reports

```
stone check
```

Fix the reported ownership (two components claiming one file) and broken links by hand or with `stone edit` and `stone link`. Repeat until `stone check` exits 0. Imported components start with the code globs Groma had; adjust them where the layout changed.

## 5. Find code no component owns

```
stone scan --dry-run
stone scan
```

Scan writes draft components under `stone/arch/scan/` for code that no component owns. Rename and describe the ones worth keeping; delete the rest.

## 6. Add the CI step

Add one step to the pipeline, after the build, that runs `stone check`. Use `stone check --strict` when component drift should fail the build too.

## 7. Optional: MCP for clients without a shell

Agents that can run a shell use the `stone` CLI and need nothing here. For a client that cannot run a shell, register `stone mcp` as its MCP server (stdio, command `stone`, argument `mcp`); it serves only the read tools `search`, `get`, `list` and `code`. Stone never turns it on by itself. Remove the Backlog.md, Groma.md and graphify servers from the same config.

## 8. Update the agent guides

In the repo's CLAUDE.md and other agent guides, replace the old tool instructions with the stone workflow. Point agents at `stone instructions` for the rules, and optionally install the commit hook:

```
stone commit-msg --install-hook
```

## 9. Verify

- `stone list` shows the imported tasks, with the counts the old board had.
- `stone search` finds a phrase you know from an old task and an old doc.
- `stone code uses` on a well-known file or symbol lists its callers.
- `stone serve` opens the board, the architecture map and the docs in the browser; open each and compare with the old tools.

## 10. Remove the old tools

Only after step 9 passes, remove in one commit: the Backlog.md folder, the Groma.md folder, graphify output and its config, the fumadocs site, and their CI steps and scripts.

## Rolling back

The imports are plain git commits. To return, revert the commits from steps 1 to 10 in reverse order. The old folders come back as they were and nothing else is affected.

## Product workspace (several repos)

Use this when the product is a folder of repos — one folder per service or app — instead of a single repo. Steps 1 to 10 above still apply, with these changes. The root folder needs no git of its own.

**1. Create the product root and init there.**

```
stone init --prefix VC
```

The root holds `stone/`, the project folders and any plain doc folders. Use your own prefix in place of `VC`.

**2. Make `stone/` its own repo.** Tasks, docs and architecture travel with their own history, apart from the code. From inside `stone/`:

```
git init
git add .
git commit -m "stone: start the store"
```

**3. List the folders in `stone/config.yml`.** `code_roots` names the project folders; `doc_roots` names plain note folders, which stone indexes for search and never writes to:

```yaml
project: shop
task_prefix: VC
code_roots: [api/, portal/, notifier/]
doc_roots: [docs/]
```

Paths stay relative to the root, so ownership, drift and links work as they do in one repo. Every project needs an owner: `stone scan` drafts one component per project folder with `code: ['<folder>/**']`; keep and rename them, or write your own.

**4. Import per old tool folder.** Run the importers of steps 2 and 3 once for each folder that had a Backlog.md or Groma.md folder, from the product root. Commit each import in the repo that holds it — `stone/`.

**5. Install the commit hook everywhere.**

```
stone commit-msg --install-hook
```

It installs into every repo it finds under the root, so agents in `api/` get the same checks as in `stone/`.

**6. Reference tasks from any project repo.** Put `Refs: VC-3` in a commit in `api/`, and `stone get VC-3` lists it under Changed files, next to commits in the other repos.

**7. Check and serve from the product root.** `stone check` covers every code root; `stone serve` watches `stone/`, each code root and each doc root. Both are run where you ran `stone init`.

**8. Rolling back.** Revert the commits in each repo separately, starting with the imports in `stone/`. Nothing outside those repos is touched.
