---
type: plan
title: M-1 store and CLI requirements
description: What `stone` does at the end of M-1, as checkable behaviour. Serves CS-2 to CS-7.
status: done
milestone: M-1
links: [CS-2, CS-3, CS-4, CS-5, CS-6, CS-7, ADR-3, ADR-4, ADR-5, ADR-10]
created: 2026-10-23
generated: { by: 'claude-code', at: 2026-10-23 }
---

## Summary

At the end of M-1 an agent plans and runs a task in this repo with `stone` alone: it finds the next task, reads one section, edits fields and sections with minimal diffs, creates items with fresh ids, links them, and `stone check` keeps the store valid in CI. Backlog.md is no longer needed here. Search, code graph, git history and MCP come in later milestones and are out of scope.

## Background

- The scaffold (core, store, index, engine, cli) builds and its tests pass, but the model still carries the old context-bundle design: `doc/` and `cmp/` id prefixes, an attachment kind, a `reviewed` stamp, bundle schema and renderers, and CLI stubs that point at deleted tasks.
- The frontmatter line-patch writer exists (`crates/store/src/frontmatter.rs`) and already keeps unchanged files byte-identical, CRLF files CRLF, and comments.
- OKF v0.2 (SPEC.md, GoogleCloudPlatform/open-knowledge-format): only `type` is required; `generated: {by, at}`; `verified` is a list of `{by, at}` and a bare map counts as one entry; actors are `human:<id>`, `process:<id>`, `<producer>[/<version>]`; trust tier is unverified, machine-confirmed (verified only by non-human actors) or human-reviewed (any `human:` verifier); concept id is the path inside the bundle without `.md`; `index.md` and `log.md` are reserved at every level; consumers tolerate unknown keys, types and broken links.

## Goals

- This repo is tracked with `stone` from the end of M-1; CI runs `stone check`.
- Every write is a minimal diff: only the lines of changed keys or sections change, and repeating a write changes nothing.

## Users and scenarios

- An agent (Claude Code, Codex) starts work: `stone task next`, `stone get CS-4#acceptance-criteria`, `stone task move CS-4 in-progress`, edits sections as it goes, writes `## Outcome`, `stone task move CS-4 done`. Failure path: moving to done without an outcome is refused with a message saying what to add.
- The lead dev plans: `stone new task --title ...`, `stone link`, `stone list --milestone M-2`. Failure path: a typo in an id gets "not found, did you mean".
- Two agents in parallel each create a task: they get different numbers and neither file is corrupted.
- CI: `stone check` fails the build on a dangling link, a bad status or a done task without an outcome.

## Scope

- In: item model for all nine types, frontmatter read and patch, store layout, lock, atomic writes, id allocation, the commands in FR-4 to FR-11, `stone check`, `stone renumber`, CI step, `stone/arch/` for this repo, rewritten `stone instructions`.
- Out: search and backlinks (M-2), `[[code:...]]` resolution (M-3), git history and trailers (M-4), MCP (M-5), web UI (M-6), importers (M-7), a `verify` command, the file store under `stone/files/`, the index crate (unchanged).
- Removed: bundle schema and renderers, the attachment kind, the `reviewed` stamp, `doc/`/`ref/`/`cmp/`/`att/` id prefixes, CLI stubs for commands outside M-1 (`search`, `timeline`, `task finalize`, `task get --context`, `index rebuild`), `--format yaml`.

## Functional requirements

### FR-1 Item types, fields and ids

Every markdown file under `stone/` except `index.md`, `log.md`, `config.yml` and `stone/files/**` is an item. Its type comes from the `type` key: doc, plan, task, milestone, decision, reference, system, container or component. Its id is its path under `stone/` without `.md` (`tasks/CS-12`, `docs/prd`, `arch/carvestone/core`). Numbered items also answer to a short id (`CS-12`, `M-3`, `ADR-7`), matched case-insensitively and ignoring zero padding, always printed in canonical form. Only the configured task prefix is a task id. Any id may carry a `#section` anchor.

Fields: common `type`, `title`, `description`, `tags`, `status`, `created`, `generated`, `verified`, `stale_after`, `links`. Task adds `priority`, `kind`, `milestone`, `plan`, `depends_on`, `assignee`, `components`, `paths`. Plan adds `milestone`. Milestone adds `target`. Decision adds `supersedes`. Reference adds `resource`, `library`, `version`. Doc adds `order`, `components`. System, container and component add `technology`, `code`, `uses`, `libraries`, `parent`. Keys not listed are kept as written.

Acceptance criteria:
- AC-1.1 Given this repo's `stone/`, when `stone list --json` runs, then every item appears once with its path id, short id where numbered, and type.
- AC-1.2 Given `cs-007`, `CS-7`, `tasks/CS-7` and `tasks/cs-7`, when each is passed to `stone get`, then all print `tasks/CS-7`.
- AC-1.3 Given task prefix `CS`, when `stone get TASK-7` runs, then it exits 1 with "TASK-7 not found" and no item is printed.
- AC-1.4 Given a file with a key Carvestone doesn't know, when any write changes another key, then the unknown key's line is unchanged.
- AC-1.5 Given `version: 1.40` on a reference, when it is read and printed as JSON, then the value is the string `"1.40"`.

### FR-2 Provenance and trust

`generated` is one `{ by, at }` map. `verified` is a list of `{ by, at }` maps; a single bare map is read as a one-entry list. Actors are `human:<id>`, `process:<id>` or `<producer>[/<version>]`. The trust tier is derived, never stored: no `verified` gives unverified, only non-`human:` verifiers give machine-confirmed, any `human:` verifier gives human-reviewed. The author of a write is `STONE_AGENT` when set (any actor form), otherwise `human:<git user.name>` lowercased with spaces as `-`, otherwise `human:unknown`. Creating an item writes `created` (date) and `generated`; an edit that changes `title`, `description` or the body rewrites `generated` (one line); status moves and links don't touch it.

Acceptance criteria:
- AC-2.1 Given `verified: { by: 'human:yaroslav', at: 2026-10-16 }`, when the item is read as JSON, then `verified` is a one-entry list and `trust` is `human-reviewed`.
- AC-2.2 Given `verified: [{ by: 'process:ci', at: 2026-10-16 }, { by: 'claude-code/opus-5.5', at: 2026-10-17 }]`, when the item is read, then `trust` is `machine-confirmed`.
- AC-2.3 Given no `verified` key, when the item is read, then `trust` is `unverified`.
- AC-2.4 Given `STONE_AGENT=claude-code`, when `stone edit CS-2 --section notes --text x` runs, then the only frontmatter line that changes is `generated`, now `{ by: claude-code, at: <today> }`.
- AC-2.5 Given `STONE_AGENT` unset and git `user.name` is `Yaroslav K`, when `stone new task --title X` runs, then `generated.by` is `human:yaroslav-k`.
- AC-2.6 Given `by: 'bot x'` (a space, no form matches), when the item is read, then `stone check` reports an invalid actor on that line.

### FR-3 Minimal-diff writes

All writes patch the file text: only the lines of keys or the body section being changed are touched; quoting, comments, key order, unknown keys and line endings elsewhere stay byte for byte. New keys go after the last frontmatter key. Lists and maps are written inline (`[a, b]`, `{ k: v }`, `[{ by: x, at: y }]`). Writing the value a file already has changes nothing. A frontmatter the restricted subset can't patch (block lists, multi-line values, anchors) is refused with the line number and nothing is written. New files use LF.

Acceptance criteria:
- AC-3.1 Given any item in this repo, when it is loaded and saved without changes, then the file is byte-identical.
- AC-3.2 Given a CRLF file, when one key is set, then every line still ends in CRLF and only that key's line differs.
- AC-3.3 Given `verified` absent, when one verification is appended, then the file gains `verified: [{ by: ..., at: ... }]`; when a second is appended, the same line holds both entries.
- AC-3.4 Given a block-style `depends_on:` list followed by `- CS-1` lines, when `stone edit` sets another key, then it exits 1 naming the line, and the file is unchanged.
- AC-3.5 Given any edit command, when it runs twice with the same arguments, then the second run prints "no change" and the file is unchanged.

### FR-4 Store, lock and atomic writes

The repo root is found by walking up from the working directory (or `--root`) to the first directory holding `stone/config.yml`. Every write, id allocation included, holds an exclusive lock on `.stone/lock` for the whole read-modify-write, and replaces the file atomically (temp file in the same folder, then rename, retried briefly when Windows reports the target busy). File names use canonical case (`CS-7.md`).

Acceptance criteria:
- AC-4.1 Given no `stone/config.yml` in any parent, when any command except `init` runs, then it exits 1 with "no stone/ found; run `stone init`".
- AC-4.2 Given 8 processes each running `stone new task` at the same time, when all finish, then there are 8 new files with 8 distinct ids, each parses, and none is truncated.
- AC-4.3 Given 2 processes editing different keys of one file at the same time, when both finish, then both changes are present.

### FR-5 Id allocation

A new numbered item gets the highest number of its kind seen in the working tree and in every local branch and remote-tracking ref (`refs/heads`, `refs/remotes`), plus one, chosen under the lock. Nothing is fetched. Outside git, or when git fails, the working tree alone counts and a warning is printed. Slug items take `--slug`, or the title as a slug; an existing path is an error.

Acceptance criteria:
- AC-5.1 Given CS-9 is the highest task in the tree and a local branch holds `stone/tasks/CS-12.md`, when `stone new task` runs, then the new task is CS-13.
- AC-5.2 Given a directory that is not a git repository, when `stone new decision --title X` runs, then it gets the next number from the tree and prints a warning that branches were not checked.
- AC-5.3 Given `docs/setup.md` exists, when `stone new doc --title Setup` runs, then it exits 1 with "docs/setup already exists" and writes nothing.

### FR-6 `stone init`

Creates `stone/config.yml` (project name from the folder, `--prefix` or `TASK`), `stone/index.md` with `okf_version: "0.2"`, and the folders `tasks`, `milestones`, `decisions`, `docs`, `plans`, `refs`, `arch`, and adds `/.stone/` to `.gitignore`.

Acceptance criteria:
- AC-6.1 Given an empty folder, when `stone init --prefix AB` runs, then the files above exist and `stone new task --title X` creates `stone/tasks/AB-1.md`.
- AC-6.2 Given an existing `stone/config.yml`, when `stone init` runs, then it exits 1 with "already initialised" and changes nothing.

### FR-7 `stone new`

`stone new <type> --title T [--slug S] [--set k=v]... [--section NAME --text T] [--dry-run]` writes a file with `type`, `title`, `status`, `created`, `generated`, the given fields, and the body sections of its type: task `## Description` and `## Acceptance criteria`; decision `## Context`, `## Decision`, `## Consequences`; system, container and component `## Responsibilities`, `## Interfaces`, `## Gotchas`; others none. Status starts at `todo` for tasks and at the first value of the type's vocabulary for the rest. It prints the new id. `--dry-run` prints the file and writes nothing.

Acceptance criteria:
- AC-7.1 Given this repo, when `stone new task --title "Lock test" --set priority=low` runs, then it prints the new id and the file has those fields, `status: todo` and the two task sections.
- AC-7.2 Given `--dry-run`, when `stone new decision --title X` runs, then the file text is printed and no file or lock change remains.
- AC-7.3 Given `--set status=bogus`, when `stone new task` runs, then it exits 1 listing the allowed statuses and writes nothing.

### FR-8 `stone edit`

`stone edit <id> [--set k=v]... [--unset k]... [--section NAME (--text T | stdin)] [--check-ac N] [--dry-run]`. For list fields (`tags`, `links`, `depends_on`, `components`, `paths`, `code`, `uses`, `libraries`) `k=a,b` replaces the list, `k=+a` adds and `k=-a` removes one entry. `--section` replaces that section's text, matched by heading case-insensitively; a missing section is appended at the end as `## <Name>`. `--check-ac N` ticks the Nth checkbox under `## Acceptance criteria`. Values in relation fields must resolve to existing items. `--dry-run` prints the resulting file and writes nothing.

Acceptance criteria:
- AC-8.1 Given CS-2, when `stone edit CS-2 --set priority=low` runs, then only the `priority` line changes.
- AC-8.2 Given `depends_on: [CS-2]`, when `--set depends_on=+CS-3` runs, then the line reads `depends_on: [CS-2, CS-3]`; running it again changes nothing.
- AC-8.3 Given a task without `## Outcome`, when `--section outcome --text "Done."` runs, then `## Outcome` with that text is appended and the other sections are byte-identical.
- AC-8.4 Given `--set depends_on=+CS-99` and no CS-99, when edit runs, then it exits 1 with "CS-99 not found" plus a did-you-mean when one is close, and writes nothing.
- AC-8.5 Given `--check-ac 2` on CS-2, when edit runs, then only the second criterion's `- [ ]` becomes `- [x]`.

### FR-9 `stone get`

`stone get <id>[#section]` prints the file as written, headed by its path. With `#section` it prints only that section: its heading line through the line before the next heading of the same or a higher level. Anchors are GitHub-style heading slugs. `--json` gives id, short id, path, type, trust, all fields, the section list (anchor, title, level, line range) and the body; with an anchor, the section's anchor, title, line range and text.

Acceptance criteria:
- AC-9.1 Given CS-2, when `stone get CS-2#acceptance-criteria` runs, then only the `## Acceptance criteria` heading and its checkboxes print.
- AC-9.2 Given `stone get CS-2#nope`, when it runs, then it exits 1 listing CS-2's anchors.
- AC-9.3 Given `--json`, when `stone get ADR-5` runs, then the output parses and holds `fields.status == "accepted"` and a `sections` entry for `decision`.

### FR-10 `stone list` and `stone task`

`stone list [--type T] [--status S] [--milestone M] [--component C] [--json]` prints one line per item (`CS-2  task  todo  high  Item model for the OKF profile`), sorted by type, then number or path. `stone task move <id> <status>` sets a task's status; moving to `done` needs a non-empty `## Outcome`. `stone task next [--limit N]` lists `todo` tasks whose `depends_on` are all done, by priority in config order, then number; default limit 5.

Acceptance criteria:
- AC-10.1 Given this repo, when `stone list --type task --milestone M-1` runs, then it prints CS-1 to CS-7 in number order and nothing else.
- AC-10.2 Given CS-3 depends on CS-2 and CS-2 is `todo`, when `stone task next` runs, then CS-2 is listed and CS-3 is not.
- AC-10.3 Given CS-2 without `## Outcome`, when `stone task move CS-2 done` runs, then it exits 1 with "CS-2 has no ## Outcome; add one with `stone edit CS-2 --section outcome`" and the file is unchanged.
- AC-10.4 Given `stone task move ADR-3 done`, when it runs, then it exits 1 saying ADR-3 is not a task.

### FR-11 `stone link`

`stone link <from> <to>...` adds each target to the source's `links` in the given order, skipping ones already there. Targets must resolve (anchors included).

Acceptance criteria:
- AC-11.1 Given CS-2 without links, when `stone link CS-2 ADR-3 docs/prd#data-model` runs, then `links: [ADR-3, docs/prd#data-model]` is added; running it again changes nothing.
- AC-11.2 Given `stone link CS-2 docs/prd#nope`, when it runs, then it exits 1 with the anchor not found and writes nothing.

### FR-12 `stone check`

Validates the whole store and prints one line per problem, `<path>:<line>: <rule>: <message>`, then a count; exits 1 when any error is found. Rules: frontmatter parses and fits the restricted subset; `type` is present and known; the folder fits the type (`tasks/` holds tasks, `arch/` systems, containers and components); numbered file names are canonical; status, kind and priority fit the config (OKF `draft|stable|deprecated` for doc, reference and arch types); actors are valid; relation fields, `links` and their anchors resolve; body links `[[id]]`, `[[id#anchor]]` and markdown links to `.md` files in `stone/` resolve (`[[code:...]]` is skipped); no two files resolve to the same id; no merge conflict markers; done tasks have a non-empty `## Outcome`; root `index.md` declares `okf_version` and other `index.md`/`log.md` files have no frontmatter; every git-tracked file under `code_roots` is owned by exactly one arch element's `code` globs. Unknown ids get a did-you-mean when one is within two edits. `--json` prints the same problems as a list.

Acceptance criteria:
- AC-12.1 Given this repo at the end of M-1, when `stone check` runs, then it exits 0.
- AC-12.2 Given one fixture per rule, when `stone check` runs on each, then each reports exactly its rule with the right path and line, and exits 1.
- AC-12.3 Given `depends_on: [ADR-11]` on a task where only ADR-1 exists, when check runs, then the message is "ADR-11 not found. Did you mean ADR-1?".
- AC-12.4 Given a file under `code_roots` matched by two components, when check runs, then both component ids are named.

### FR-13 `stone renumber`

`stone renumber <path> [--dry-run]` gives the numbered item at `<path>` the next free number and renames the file. When no other file holds the old id, links to it are rewritten everywhere; when one does (a real collision), links are left alone and every item linking to the old id is listed for review.

Acceptance criteria:
- AC-13.1 Given `tasks/CS-07.md` and no `CS-7.md`, when `stone renumber stone/tasks/CS-07.md` runs, then the file gets the next free number and every `[CS-7]`/`[[CS-7]]` reference points at the new id.
- AC-13.2 Given `tasks/CS-7.md` and `tasks/CS-007.md`, when the second is renumbered, then it gets a new id, links are unchanged, and the items linking to CS-7 are listed.

### FR-14 CI, dogfood and instructions

CI runs `stone check` on all three OSes after the tests. `stone/arch/` describes this repo: one system for Carvestone and one component per crate with `code` globs. `stone instructions` prints a short guide to the M-1 workflow.

Acceptance criteria:
- AC-14.1 Given the CI workflow, when it runs, then a step runs `stone check` and fails the job on a non-zero exit.
- AC-14.2 Given `code_roots: [crates/]`, when `stone check` runs, then every tracked file under `crates/` is owned by exactly one component.
- AC-14.3 Given `stone instructions`, when it runs, then it names only commands that exist (next, get, move, edit, new, link, list, check) and fits in 25 lines.

## Non-functional requirements

- Platforms: Windows, macOS, Linux; CI on all three.
- Speed on this repo: reads under 300 ms, `check` under 1 s, `new` under 1 s with git.
- Offline; no network ever.
- Rust 1.89, edition 2024, `unsafe_code = "forbid"`. New dependencies only `globset`.
- Errors: one line, start with `✗`, name what to do next; exit 1 for errors, 2 for usage.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Remote ids not fetched lead to a cross-machine collision | Two files claim CS-25 after a merge | Medium | Add/add conflict shows in git; `check` and `renumber` resolve it |
| Lock left by a killed process | Writes hang | Low | OS releases `File::lock` on exit; no lock files to clean |
| Windows rename fails while an editor holds the file | Write lost | Medium | Retry, then a clear error; the temp file is removed |

## Alternatives considered

- OKF `status` plus a separate workflow `state:` key: strictly OKF, but rewrites every file and the PRD. Rejected by the owner.
- A thin slice without lock, ids and check: faster swap from Backlog.md, but concurrency bugs found in use. Rejected by the owner.
- A shared id counter in the git common dir: covers worktrees, not branches or clones; the branch scan covers both.

## Open questions

None.

## Decision log

- 2026-10-23 Requirements live in `stone/plans/` as a plan item, not `docs/`, because this repo keeps its knowledge in `stone/` - claude-code
- 2026-10-23 Order: model, writer, store, check, write commands, renumber and CI, dogfood - owner
- 2026-10-23 Workflow status stays in `status` with per-type vocabularies - owner
- 2026-10-23 `stone edit` takes `--set`, `--unset`, `--section`, `--check-ac` - owner
- 2026-10-23 Id scan reads local and remote-tracking refs, never fetches - claude-code
- 2026-10-23 New tasks start at `todo`; no `verify` command in M-1 - claude-code
- 2026-10-23 `renumber` rewrites links only when the old id is left unused - claude-code
