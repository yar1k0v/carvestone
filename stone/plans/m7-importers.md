---
type: plan
title: M-7 importers and switch-over requirements
description: The rest of the Backlog.md importer, the Groma.md importer, dry-run and idempotency, and the switch-over checklist, as checkable behaviour. Serves the M-7 task list.
status: done
milestone: M-7
links: [M-7, M-2, ADR-3, ADR-8, ADR-9, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-07 }
---

## Summary

At the end of M-7 a repo that runs on Backlog.md and Groma.md moves to Carvestone with two commands that can be repeated: `stone import backlog backlog/` brings milestones, decisions, tasks and docs over with ids kept, and `stone import groma groma/` turns elements into `arch/` items with `code` globs and `uses`. Both support `--dry-run`, a second run changes nothing, and anything edited in stone since is never overwritten. A checklist doc covers the Freightline switch; the switch itself is the owner's call and not run by this plan. The Freightline repo stays out of this repo.

## Background

- M-2 shipped the first Backlog.md importer for tasks and docs (`engine/src/import/backlog.rs`): status mapping, section mapping, the shared `import::apply` with its stored-hash skip rule (`.stone/import-state.json`), and golden-file tests. M-7 adds the rest and a second importer on the same write path.
- ADR-8: Groma.md keeps relations in markdown tables and nests ids under a `groma:` key; our format is flat, one file per element, with `code` globs and `uses`. ADR-3: not compatible with either tool, importers bring data over. ADR-9: both tools are replaced, graphify needs no importer, Docs MCP stays.
- Roadmap: Groma format variance is the risk; fixtures are frozen synthetic or open-source samples in `bench/`; Freightline numbers stay local; the Freightline switch waits for the UI.
- The Backlog.md and Groma.md layouts below are the formats as this plan understands them; fixtures from a real Freightline copy decide the final field names before T1 and T2 start.

## Goals

- Re-running an import on an unchanged source gives no diff; running it after a human edit never loses the edit.
- Every imported file passes `stone check` (frontmatter, vocabulary, links, ownership) with no manual cleanup.
- Nothing is written when any part of the input cannot be mapped.

## Users and scenarios

- The lead dev runs `stone import backlog backlog/ --dry-run`, reads the list of new files, then runs it for real, then runs `stone check`. Failure path: a status `Blocked` that the config does not allow stops the import with the value, the file and the allowed list; nothing is written.
- The same for `stone import groma groma/`. Failure path: a relationship names an element that does not exist; the import stops listing file, line and the name.
- A week later the source changed: the import runs again; changed sources update, unchanged files stay, and a task the dev edited in stone is skipped with a message.
- The dev wants out of a bad import: `git restore`-style undo is git's job; the importer touches only files it created and leaves the rest of `stone/` alone.

## Scope

- In: Backlog.md milestones, decisions, the remaining task fields (sub-tasks, drafts, archive, references), prefix handling; the Groma.md importer; shared plan/apply run with dry-run and guard; the switch checklist doc.
- Out: running the switch on Freightline, removing old tool folders (the owner does it), fumadocs content (docs come through Backlog or Groma folders; a plain markdown folder import is not built), graphify, round-trip export back to the old tools, merging an import into an existing hand-written item.

## Functional requirements

### FR-1 Backlog.md milestones and decisions

`stone import backlog <dir>` also reads `<dir>/milestones/*.md` and `<dir>/decisions/*.md`. A milestone `m-<n>` becomes `stone/milestones/M-<n>.md` (`type: milestone`, `title`, `status` from its state mapped to the milestone vocabulary, body kept as written under no headings change). A decision `decision-<n>` becomes `stone/decisions/ADR-<n>.md` (`type: decision`, `status` mapped case-insensitively to the decision vocabulary, sections Context, Decision, Consequences matched by heading case-insensitively; other sections kept after them). `date` becomes `created`. Order of the run is milestones, decisions, tasks, docs, so `milestone` and `links` on tasks resolve inside the same run. A status outside the type's vocabulary stops the import (see FR-5).

Acceptance criteria:
- AC-1.1 Given `milestones/m-2 - Search.md`, when imported, then `stone/milestones/M-2.md` has `type: milestone` and `title: Search`.
- AC-1.2 Given `decisions/decision-3 - Use SQLite.md` with status `Accepted`, when imported, then `stone/decisions/ADR-3.md` has `status: accepted` and the three sections.
- AC-1.3 Given a task with `milestone: m-2`, when imported in the same run, then its `milestone` is `M-2`.
- AC-1.4 Given a decision body with an extra `## Alternatives` section, when imported, then it follows `## Consequences` unchanged.
- AC-1.5 Given a decision status `Pending review` not in the vocabulary, when imported, then exit 1 naming the value, the file and the allowed list; nothing is written.

### FR-2 Backlog.md tasks, completed

On top of M-2: `<dir>/drafts/` tasks get `status: draft` when the vocabulary has it, else the import stops (FR-5); `<dir>/archive/**` get `status: archived` likewise; `parent_task_id` becomes a `links` entry to the parent (sub-tasks stay separate items); `references` entries that are URLs go to `links` when they resolve to items and otherwise to a `## Notes` bullet list; acceptance criteria keep their checked state (`- [x]` stays `- [x]`) and the `<!-- AC:BEGIN -->` style markers are dropped; `ordinal` is dropped; unknown frontmatter keys are kept as written (ADR-3). The prefix is `--prefix`, else the Backlog config's prefix, else the stone config's task prefix; the first non-empty wins, and numbers are kept.

Acceptance criteria:
- AC-2.1 Given acceptance criteria with two checked items, when imported, then the same two boxes are `[x]` and markers are gone.
- AC-2.2 Given `parent_task_id: task-4`, when imported, then `links` holds `VC-4`.
- AC-2.3 Given a draft task and a vocabulary with `draft`, when imported, then `status: draft`.
- AC-2.4 Given `--prefix` absent and a Backlog config prefix `VC`, when imported, then files are named `VC-n.md`.
- AC-2.5 Given an unknown key `foo: 1`, when imported, then the line is in the output unchanged.

### FR-3 Groma.md elements

`stone import groma <dir>` reads each element file under `<dir>` (frontmatter `groma: { id, type, parent?, code? }` plus title/description keys, body sections) and writes `stone/arch/**`. Types map `system` to system, `container` to container, `component` to component; `person` and external systems become systems with `tags: [person]` or `tags: [external]`. The file path follows the parent chain: a root element is `arch/<slug>.md`, its children `arch/<slug>/<slug>.md`, deeper levels nest likewise; `<slug>` is the source file name without extension, lowercased, unsafe characters as `-`. Frontmatter gets `type`, `title`, `description`, `parent` (the parent's arch id), `technology` when present, `status` (`draft` when Groma marks the element a draft, else `stable`), `created` (source date or the import date on first import), `generated: { by: 'process:import-groma', at }`.

Acceptance criteria:
- AC-3.1 Given a container `api` with parent system `shop`, when imported, then `stone/arch/shop/api.md` has `type: container` and `parent: arch/shop`.
- AC-3.2 Given a draft element, when imported, then `status: draft`.
- AC-3.3 Given an element of type `person`, when imported, then it is a system with `tags: [person]`.
- AC-3.4 Given two elements with the same slug under one parent, when imported, then exit 1 "arch/shop/api.md would be written twice" and nothing is written.
- AC-3.5 Given a parent id that does not exist, when imported, then exit 1 naming the file and the id.

### FR-4 Groma.md code and relationships

`groma.code` becomes `code` globs: a path ending in `/` becomes `<path>**`, a path with glob characters is kept, a file path is kept; separators become `/`; the list is sorted and deduplicated. Relationship tables (a markdown table under a heading that contains "relationship" or "uses", first column the target, other columns description and technology) become `uses` entries pointing at the target's arch id, resolved by Groma id, then by title, then by slug; each row's description is added as a bullet `- arch/<target>: <description>` under `## Interfaces`. A target that does not resolve stops the import (FR-5) naming file, line and name. `libraries` come from a `libraries` key or a heading `Libraries` list when present.

Acceptance criteria:
- AC-4.1 Given `code: [src/Api/]`, when imported, then `code: [src/Api/**]`.
- AC-4.2 Given a table row `Postgres | stores orders | SQL`, when imported with that element present, then `uses: [arch/shop/postgres]` and a bullet in `## Interfaces`.
- AC-4.3 Given a row naming `Nope`, when imported, then exit 1 with `<file>:<line>: relationship target "Nope" not found` and nothing is written.
- AC-4.4 Given backslash paths, when imported, then globs use `/`.
- AC-4.5 Given the same relationship listed twice, when imported, then one `uses` entry.

### FR-5 Run semantics for both importers

Both importers go through `import::apply` (M-2). It takes the store lock first, then re-reads sources and targets, builds the full plan in memory (every target path with its final text), checks it (vocabulary, links, ids, unique paths) and writes the files whose text differs. Any error means exit 1 and no file written. Edit detection is the M-2 stored hash: a target is overwritten only when its current text hashes to the value in `.stone/import-state.json` (shared by both importers; `generated.by` is not used, because status moves and link edits leave `generated` alone); otherwise it is skipped with `skipped <id>: edited in stone`, and the same holds when the state file is missing. The state file is updated before the lock is released. `--dry-run` (no lock, read-only) prints `new`, `changed`, `unchanged` or `skipped` per path and writes nothing. Output ends with counts (`3 new, 0 changed, 40 unchanged, 1 skipped`). The importers never delete a file, even when its source disappeared (they list `source gone: <path>`). Same input twice gives byte-identical files; `generated.at` is the source's date, or the first import's date, never the current date on a re-run.

Acceptance criteria:
- AC-5.1 Given any fixture, when imported twice, then the second run reports `0 new, 0 changed` and the tree is byte-identical.
- AC-5.2 Given a source change in one task, when re-imported, then exactly that file changes.
- AC-5.3 Given an imported Groma element changed only by `stone edit --set status=stable` (`generated` untouched), when re-imported, then it is skipped and the file is unchanged.
- AC-5.4 Given `--dry-run`, when run, then no file or lock change remains.
- AC-5.5 Given a removed source file, when re-imported, then the target stays and `source gone: <path>` prints.
- AC-5.6 Given 8 concurrent imports, when all end, then the files are valid and equal to a single run.
- AC-5.7 Given the golden output, when `stone check` runs on it, then exit 0.

### FR-6 Switch-over checklist

`stone/docs/switch-over.md` (type doc) lists, in order, the steps to move a repo: install and `stone init --prefix`; `import backlog --dry-run`, then for real; `import groma`; `stone check` and fixing ownership and links; `stone scan` for unowned code; adding the CI step; registering `stone mcp`; updating the repo's CLAUDE.md and agent guides; removing `backlog/`, `groma/`, graphify output, fumadocs and their CI steps; a verification list (`stone list`, `stone search`, `stone code uses`, the board in `stone serve`); and rollback (the imports are plain git commits, revert them). It names no Freightline paths.

Acceptance criteria:
- AC-6.1 Given the doc, when `stone check` runs, then it passes (links resolve, frontmatter valid).
- AC-6.2 Given every command in backticks in the doc, when compared with clap subcommands and flags, then each exists (a test).

## Non-functional requirements

- Platforms: Windows, macOS, Linux; CRLF sources are read, output is LF.
- A 2,000-item import in under 10 s; memory proportional to the plan.
- Offline. Rust 1.89. No new dependencies beyond a markdown table reader built on the core scanners (no second fence or code-span parser).
- Errors are plain sentences with file and line; only the CLI adds `✗`.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Groma files vary between projects | Wrong or failed imports | High | Fixtures from real files first, strict stop-on-unknown for structure, unknown keys and sections kept verbatim |
| Backlog.md field names differ by version | Dropped data | Medium | Golden fixtures per version seen; unknown keys kept |
| Re-import clobbers edits | Lost work | Low | Stored-hash guard, tests for it |
| Status vocabularies differ from config | Import stops | Medium | Message lists the allowed values; config can be extended before the run |
| Partial write after a crash | Half-imported tree | Low | Plan first, write under the lock; re-run converges |

## Alternatives considered

- Merging an import into hand-written items: needs field-level conflict rules; skipping edited items is simpler and safe.
- Deleting targets whose source vanished: destructive; listing is enough.
- A generic markdown-folder importer for fumadocs: its content arrives through the other two; build it only if a real folder needs it.

## Open questions

None.

## Decision log

- 2026-10-25 Sub-tasks stay separate items linked to the parent - claude-code
- 2026-10-25 Persons and external systems become systems with tags - claude-code
- 2026-10-25 Importers never delete targets - claude-code
- 2026-10-25 `generated.at` is the source date, so a re-run is byte-identical - claude-code
- 2026-10-25 Edit detection is the stored hash of the last written text, shared with M-2, not `generated.by` - claude-code
- 2026-10-25 Importers lock first, then re-read, compute and write (M-1 order); dry-run reads without the lock - claude-code
- 2026-10-25 The Groma task is split into elements and paths, then relationships and code globs - claude-code

## Tasks

### T1 Backlog.md milestones, decisions and remaining task fields

Scope: FR-1 and FR-2 on the existing planner; about 300 lines.
Owned files: `crates/engine/src/import/backlog.rs`, `crates/engine/tests/import_backlog_full.rs`, `crates/engine/tests/golden/backlog-full/**`, `bench/import/backlog-full/**`.
Interfaces: `plan_backlog`, `import_backlog`, `apply`, `ImportOpts`, `ImportPlan`, `PlannedFile` (existing from M-2); milestones and decisions are added to the plan before tasks and docs.
Tests: `milestone_becomes_m_n` (AC-1.1), `decision_status_and_sections` (AC-1.2), `milestone_ref_resolves_in_run` (AC-1.3), `extra_section_kept_after_consequences` (AC-1.4), `bad_decision_status_stops` (AC-1.5), `checked_criteria_kept` (AC-2.1), `parent_becomes_link` (AC-2.2), `draft_status` (AC-2.3), `prefix_from_backlog_config` (AC-2.4), `unknown_key_kept` (AC-2.5). Invariants: the same input twice gives the same bytes; the plan is complete before any write.
Finish line: `cargo test -p carvestone-engine import_backlog_full` passes.
Depends on: M-2 done.
Risk: normal.

### T2 Groma.md elements, types and paths

Scope: parse element files, map types and nesting, write frontmatter and kept sections (FR-3); about 300 lines.
Owned files: `crates/engine/src/import/groma.rs`, `crates/engine/src/import/mod.rs`, `crates/engine/tests/import_groma_elements.rs`, `crates/engine/tests/golden/groma/**`, `bench/import/groma/**`, `crates/cli/src/main.rs`.
Interfaces: `pub fn import_groma(repo: &Repo, dir: &Path, opts: &ImportOpts) -> Result<ImportReport, Error>`; `pub fn plan_groma(repo: &Repo, dir: &Path) -> Result<ImportPlan, Error>`; `fn parse_element(text: &str, path: &str) -> Result<GromaElement, Error>`; `GromaElement { id, kind, parent: Option<String>, title, description, code: Vec<String>, relations: Vec<Relation>, sections: Vec<(String, String)>, draft: bool }`; `Relation { target: String, description: String, line: u32 }`; CLI `stone import groma <dir> [--dry-run]` (new); `import/mod.rs` gains the `groma` module and the subcommand wiring.
Tests: `container_nests_under_system` (AC-3.1), `draft_status` (AC-3.2), `person_is_tagged_system` (AC-3.3), `duplicate_slug_stops` (AC-3.4), `missing_parent_stops` (AC-3.5), `flow_and_draft_sections_kept_verbatim`. Invariant: sections not mapped are kept in source order.
Finish line: `cargo test -p carvestone-engine import_groma_elements` passes.
Depends on: T1.
Risk: high (format variance; start from real fixture files the owner supplies).

### T3 Groma.md code globs and relationships

Scope: `groma.code` to globs, relationship tables to `uses` and `## Interfaces` bullets, libraries (FR-4); about 250 lines.
Owned files: `crates/engine/src/import/groma_relations.rs`, `crates/engine/src/import/table.rs`, `crates/engine/src/import/groma.rs`, `crates/engine/tests/import_groma_relations.rs`.
Interfaces: `fn parse_table(text: &str) -> Vec<Vec<String>>` (built on the core scanners); `fn code_globs(raw: &[String]) -> Vec<String>`; `fn resolve_relations(elems: &[GromaElement]) -> Result<Vec<ResolvedRelation>, Error>`.
Tests: `dir_becomes_glob` (AC-4.1), `relationship_becomes_uses_and_bullet` (AC-4.2), `unknown_target_stops_with_line` (AC-4.3), `backslashes_normalised` (AC-4.4), `duplicate_relation_deduped` (AC-4.5).
Finish line: `cargo test -p carvestone-engine import_groma_relations` passes.
Depends on: T2.
Risk: high.

### T4 Dry-run, idempotency and guard tests for both importers

Scope: shared end-to-end tests over both fixtures (FR-5); about 250 lines.
Owned files: `crates/engine/tests/import_run.rs`, `crates/engine/src/import/mod.rs`.
Interfaces: none new; fixes found go into `apply` (existing).
Tests: `import_twice_no_diff` (AC-5.1), `one_source_change_one_file` (AC-5.2), `status_edit_makes_target_skipped` (AC-5.3), `dry_run_leaves_nothing` (AC-5.4), `source_gone_listed_not_deleted` (AC-5.5), `eight_concurrent_imports_equal_one` (AC-5.6), `imported_tree_passes_check` (AC-5.7).
Finish line: `cargo test -p carvestone-engine import_run` passes.
Depends on: T3.
Risk: normal.

### T5 Switch-over checklist

Scope: one doc and a command-existence test (FR-6); about 150 lines.
Owned files: `stone/docs/switch-over.md`, `crates/cli/tests/switch_doc.rs`.
Interfaces: none; the test reads the doc and clap's command tree (existing).
Tests: `doc_commands_exist` (AC-6.2); AC-6.1 is the repo's own `stone check`.
Finish line: `cargo test -p carvestone-cli switch_doc` passes and `cargo run -q --bin stone -- check` exits 0.
Depends on: T4 and M-6 done (the checklist mentions the board and `serve`).
Risk: normal.
