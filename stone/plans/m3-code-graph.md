---
type: plan
title: M-3 code graph and architecture requirements
description: Symbol extraction, per-language resolvers, code queries, modules, drift and scan, as checkable behaviour. Serves the M-3 task list.
status: done
milestone: M-3
links: [M-3, ADR-1, ADR-7, ADR-8, ADR-10, ADR-11, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: claude-code, at: 2026-11-07 }
---

## Summary

At the end of M-3 an agent asks `stone code find|uses|deps|impact|path|explain` for C#, Rust, TypeScript/JavaScript and Python and gets budgeted hits marked `exact` or `inferred`. `[[code:...]]` links resolve in `check`, `get` and search; symbols are searchable. Architecture gets its graph: dependencies between components, drift against `uses` in `check`, Louvain modules, and `stone scan` writing draft components. Each language reaches 90% usages recall on a frozen sample. Git, MCP-other-than-`code`, UI and importers are out of scope.

## Background

- ADR-11: tree-sitter, a resolver per language, `exact` only when resolved without the name-only fallback, Louvain within project and folder boundaries, the graph lives in the index and is never committed. ADR-8: scans never edit curated fields or text and running one twice changes no files.
- Roadmap: `graph` is a leaf crate that depends on core only and never touches SQLite; engine joins graph and index. Schema v3 adds symbols and edges to the M-2 index (rebuilt on bump). Refresh (`engine/src/refresh.rs`, M-2) is the single writer; the graph is updated in its batched transactions (M-2 `in_batches`, 200 files each, resumable).
- M-2 stores `[[code:...]]` links as kind `code` and `check` skips them. `check` already requires every tracked file under `code_roots` to have exactly one owner.
- Resolver order for every language: local scope, imports, same namespace or module, project-wide name match (the last is always `inferred`).
- Open items from the roadmap: the user captures IDE find-usages per language sample and picks open-source or synthetic fixtures; check MSRV of tree-sitter and grammar crates against Rust 1.89.

## Goals

- Usages recall of at least 0.9 per language on its frozen sample, enforced in CI; Freightline numbers are measured locally.
- Incremental update of one changed file in under 1 s; full index of a 2,000-file synthetic C# tree in a CI-friendly time; Freightline full index under 60 s locally.
- The incremental graph always equals the graph built from scratch for the same tree.

## Users and scenarios

- An agent asks `stone code uses OrderService.Place` before changing it and sees callers with `exact`/`inferred`, then `impact` for components, tasks and docs. Failure path: `uses Place` matches three symbols; the error lists the qualified names and exits 1.
- A person renames a method in an editor; the next `stone code uses` is current without any command. Failure path: a file that does not parse still yields the symbols before the error and a `parse-error` warning, never a crash.
- The lead dev runs `stone check` after adding an import across components: `arch-drift` names both components and the first import site. Failure path: the graph is unavailable (no code files); drift is skipped silently, exit 0.
- `stone scan` proposes components for unowned modules; running it twice leaves files byte-identical, and a draft the owner edited is never touched.

## Scope

- In: `crates/graph` (extract, resolvers, Louvain), schema v3, graph in refresh, code queries and CLI, code links, symbols in search, component graph and drift, modules, scan, benches, MCP `code` tool.
- Out: git history (M-4), HTTP and UI map (M-6), compiler or LSP resolution, languages beyond the four, call-graph through reflection or dynamic dispatch, generated code under `bin/` `obj/` `node_modules/` `target/`.

## Functional requirements

### FR-1 Extraction

`graph::extract(lang, path, src) -> FileFacts` parses with tree-sitter and returns declarations `{name, qname, kind, parent, signature, doc, line_start, line_end}` with kind in `project, module, file, type, function, method, member`, plus reference sites `{name, qualifier, kind (imports|calls|inherits|implements|references), line, scope}` and imports. Doc is the leading comment block. `Lang` is chosen by extension (`.cs`; `.rs`; `.ts .tsx .js .jsx .mjs .cjs`; `.py`). Project nodes come from `*.csproj`, `Cargo.toml`, `package.json`, `pyproject.toml`. A parse error keeps every declaration outside the broken region and adds a warning with the line.

Acceptance criteria:
- AC-1.1 Given a C# file with a namespace, a partial class and two methods, when extracted, then it yields module, type and two method declarations with signatures and line ranges.
- AC-1.2 Given a doc comment above a method, when extracted, then `doc` holds its text without comment markers.
- AC-1.3 Given a file with a syntax error in the last method, when extracted, then earlier declarations are present and one warning names the line.
- AC-1.4 Given the same bytes twice, when extracted, then the facts are equal.

### FR-2 Index schema v3

`SCHEMA_VERSION` becomes 3 (rebuild on bump with the M-2 lock-file protocol, ADR-1). Tables: `symbols(id, file_id, qname, name, kind, parent_id, signature, doc, line_start, line_end, lang, project)`; `refs(id, file_id, src_symbol_id, name, qualifier, kind, line)`; `edges(src_id, dst_id, dst_name, kind, exact, file_id, line)` where `dst_id` is null for an unresolved reference; `symbols_fts(name, qname, signature, doc)`; `modules(id, project, label, file_id)`. `files` gains `lang`. Deleting a file's row cascades to its symbols, refs and edges.

Acceptance criteria:
- AC-2.1 Given a v2 db, when opened, then it is rebuilt at v3 with empty code tables.
- AC-2.2 Given a file row deleted, when read back, then no symbols, refs or edges remain for it.
- AC-2.3 Given a v2 db and 4 threads opening it at once, when all return, then none failed and the db is at v3 (the M-2 concurrent-bump test, rerun with the v3 constant).

### FR-3 C# resolver

Per file: the namespace (block or file-scoped), `using`, `global using`, `using static`, aliases; per project: `ProjectReference` edges (kind `imports`, exact), partial classes merged under one qname, base lists as `inherits`/`implements`. A reference resolves in the order local scope, usings and namespace, project references, then the name-only fallback. A member call `x.M()` is exact when the receiver's type is declared (a typed local, parameter or field, `this`, `new T()`, `var x = new T()`) and `T` has `M`; otherwise inferred by name.

Acceptance criteria:
- AC-3.1 Given `using Shop.Orders;` and a call `new OrderService()`, when resolved, then the edge is exact to `Shop.Orders.OrderService`.
- AC-3.2 Given `var s = new OrderService(); s.Place()`, when resolved, then the `Place` edge is exact.
- AC-3.3 Given a call on an untyped parameter `x.Place()`, when resolved, then edges go to every `Place` and are inferred.
- AC-3.4 Given a class declared `partial` in two files, when indexed, then one type symbol exists with both files' members.
- AC-3.5 Given a `ProjectReference`, when indexed, then an exact `imports` edge exists between the two projects.

### FR-4 Graph in refresh

Refresh hashes code files (git tracked plus untracked and not ignored, under `code_roots`, supported extensions) and project files (`*.csproj`, `Cargo.toml`, `tsconfig.json`, `package.json`, `pyproject.toml`, stored with `lang = project`). For a changed code file it replaces that file's symbols and refs; for a changed, added or removed project file it re-resolves every file of the projects it governs (nearest project root; project facts feed import tables, crate names and path maps). It also re-resolves every file whose refs name a symbol that was added, removed or changed in the changed files (match by name), plus edges whose target was deleted. Facts are parsed in parallel, written serially in `in_batches` transactions of at most 200 files; `meta.code_building` is 1 until the pass ends and code queries then print "code index is still building (n of m files); results may be incomplete". Invariants: a second refresh writes nothing; `symbols`, `refs` and `edges` equal those of a full rebuild of the same tree after any sequence of edits, project-file edits included (order-independent); deleting a file leaves no edge pointing to it; a pass killed after any batch resumes and ends equal to an uninterrupted one.

Acceptance criteria:
- AC-4.1 Given an indexed tree, when one file adds a type `Foo` that another file referenced by name, then the old inferred edge is replaced by the exact one in the same refresh.
- AC-4.2 Given a random edit sequence on a fixture that includes edits of a `.csproj`, a `Cargo.toml` and a `tsconfig.json`, when each step is refreshed and compared with a rebuild, then they are equal after every step.
- AC-4.5 Given a `tsconfig.json` `paths` entry changed to point elsewhere, when refreshed, then edges that used it are re-resolved with no source file touched.
- AC-4.6 Given a fill stopped after its second batch, when refresh runs again, then the rows equal an uninterrupted run, and a query in between shows the "still building" note.
- AC-4.3 Given one changed file in a 2,000-file synthetic tree, when refreshed (Linux only), then it takes under 1 s.
- AC-4.4 Given no change, when refreshed, then zero rows are written.

### FR-5 `code find`, `uses`, `deps`

`stone code find <name> [--kind K]` lists symbols by qualified name, exact name or prefix. `uses <symbol|file>` lists incoming `calls, imports, inherits, implements, references` edges; `deps <symbol|file>` lists outgoing ones. A target is a qualified name, a unique plain name, a path, or `path:line`. Hits are the shared shape with `exactness`; the text line is `<qname|path>  <path>:L<a>-<b>  <kind>  <exact|inferred>  <signature or edge>`. Flags `--exact`, `--limit`, `--budget`, `--json`. An ambiguous target is an error listing up to 10 candidates.

Acceptance criteria:
- AC-5.1 Given `OrderService.Place` with one exact and one inferred caller, when `uses` runs, then both appear, marked, exact first.
- AC-5.2 Given `--exact`, when run, then the inferred caller is absent.
- AC-5.3 Given `Place` matching 3 symbols, when `uses Place` runs, then it exits 1 with "Place matches 3 symbols; use one of: ...".
- AC-5.4 Given a file path, when `deps` runs, then it lists the files and symbols it imports or calls.
- AC-5.5 Given `--budget 100`, when run, then the cost is at most 100 (chars/4) and `truncated` is true in JSON.

### FR-6 `impact`, `path`, `explain`

All three take `--limit`, `--budget` and `--json` like FR-5; every symbol, file or step they list is a shared-shape hit with `exactness`, and a `path` result also carries `exact: bool` (true only when every step is exact) and the step list in order. `impact <target> [--depth N]` walks incoming edges transitively (default depth 3, petgraph) and reports dependent symbols and files, the components owning them, open tasks whose `components` or `paths` cover them, and docs with `components` or `[[code:...]]` links to them. `path <a> <b>` finds the shortest edge path, trying a to b, then b to a, then undirected (labelled `undirected`). `explain <module|component>` prints files, up to 10 key symbols by inbound degree, inbound and outbound dependencies, docs and tasks.

Acceptance criteria:
- AC-6.1 Given A calls B calls C, when `impact C --depth 2` runs, then A and B are listed with their depths.
- AC-6.2 Given a cycle, when `impact` runs, then it terminates and lists each node once.
- AC-6.3 Given C owned by component X with an open task listing X in `components`, when `impact C` runs, then X and the task appear.
- AC-6.4 Given two unconnected symbols, when `path` runs, then it prints "no path between A and B" and exits 0.
- AC-6.5 Given a component id, when `explain` runs, then output holds its files, dependencies and tasks.
- AC-6.6 Given `impact` over 30 dependents and `--budget 100`, when run, then the cost is at most 100, `truncated` is true, and each listed dependent shows `exact` or `inferred`.
- AC-6.7 Given a `path` through one inferred edge, when run, then `exact` is false and that step is marked `inferred`.

### FR-7 Code links and symbols in search

`[[code:Qualified.Name]]` resolves to one symbol (exact qname, else unique name); `[[code:path/file.ext:a-b]]` to a tracked file whose line count covers `a-b`. `check` reports `code-link` problems with the file and line (`Qualified.Name not found`, ambiguity list, `range 120-160 is past the end of the file (90 lines)`); an unresolved link fails `check` only when the code graph has symbols for that language. Resolved links appear in `links` and as backlinks of the symbol: `code find`, `get` of the doc and `code explain` show docs and tasks that mention it. Symbols are searchable: hits have `kind: symbol`, `id` the qname, `path`, `lines` and the signature as snippet.

Acceptance criteria:
- AC-7.1 Given a doc with `[[code:Shop.Orders.OrderService]]`, when `stone get` of the symbol's file runs, then the doc is listed as a backlink.
- AC-7.2 Given `[[code:Shop.Nope]]` on line 14 of `stone/docs/x.md` and C# symbols indexed, when `check` runs, then stdout holds exactly `stone/docs/x.md:14: code-link: Shop.Nope not found` and the exit code is 1.
- AC-7.3 Given `[[code:src/a.cs:10-200]]` on a 50-line file, when `check` runs, then the message names 50 lines.
- AC-7.4 Given the query `OrderService`, when `search` runs, then a `symbol` hit with its signature appears.

### FR-8 Component graph and drift

File ownership comes from component `code` globs (existing). A component edge A to B exists when at least one exact or inferred code edge goes from a file owned by A to a file owned by B (A not B; drafts count as components). `get` of a system, container or component shows `Code dependencies:` with B, edge count and the first site, next to the curated `uses`. `check` prints warnings, which do not change the exit code (`--strict` makes warnings fail): `arch-drift: arch/x depends on arch/y in code (12 edges, first src/a.cs:30) but does not list it in uses` and `arch-drift: arch/x lists arch/y in uses but no code edge exists`. Drift only counts edges whose both ends are owned.

Acceptance criteria:
- AC-8.1 Given an import from A's file to B's file and `uses: []`, when `check` runs, then the first message appears for A and exit is 0.
- AC-8.2 Given the same with `--strict`, when run, then exit is 1.
- AC-8.3 Given `uses: [arch/b]` and no code edge, when `check` runs, then the second message appears.
- AC-8.4 Given only inferred edges, when drift is computed, then they count (a note says `inferred only`).

### FR-9 Rust

Cargo workspace members and crate names from `Cargo.toml`; the module tree from `mod x;` (`x.rs`, `x/mod.rs`; `#[path]` makes the module's edges inferred); `use` paths with `crate::`, `self::`, `super::`, nested groups, aliases, `pub use` re-export chains and glob imports; `impl` blocks attach methods to their type; trait impls give `implements`. Path calls (`Type::f`, `module::f`) resolve exact; `self.f()` inside an `impl` is exact; other method calls are inferred.

Acceptance criteria:
- AC-9.1 Given `use crate::store::Store; Store::open()`, when resolved, then the call edge is exact.
- AC-9.2 Given `pub use inner::Thing` in `lib.rs` and `use mycrate::Thing`, when resolved, then the edge reaches `inner::Thing`.
- AC-9.3 Given `x.open()` with x of unknown type, when resolved, then edges are inferred.
- AC-9.4 Given `impl Display for Foo`, when indexed, then an `implements` edge Foo to `Display` exists.

### FR-10 TypeScript/JavaScript and Python

TS/JS: relative specifiers and `tsconfig.json` `baseUrl` and `paths`, extension and `index.*` resolution, named, default, namespace imports, `export ... from`, `export *`, literal `require`; `node_modules` and bare package imports stay unresolved (no edge). Python: packages via `__init__.py`, namespace folders and `src/` layouts, `import a.b`, `from a import b`, relative imports, `pyproject.toml` projects; `self.m()` in a class is exact. Member and attribute calls are otherwise inferred, calls through an imported binding are exact.

Acceptance criteria:
- AC-10.1 Given `import { f } from '@app/util'` with a `paths` entry `@app/*`, when resolved, then the call `f()` is exact to the file's export.
- AC-10.2 Given `export * from './a'` in `index.ts` and `import { x } from './dir'`, when resolved, then x resolves through the star export.
- AC-10.3 Given `from pkg.mod import run; run()`, when resolved, then the edge is exact.
- AC-10.4 Given `obj.run()` on an untyped `obj`, when resolved, then edges are inferred.

### FR-11 Modules

Louvain (deterministic: fixed node order by file path, no randomness, resolution 1.0) runs per group of files sharing a project and first-level folder under the project root, over file-to-file edge weights (exact 1.0, inferred 0.3); a module never spans two groups. Module id is `mod:<project>/<folder>/<k>` where `k` ranks modules by size descending, then by smallest file path; its label is the longest common folder of its files. Results are stored in `modules` with `meta.modules_version`. `update_code` bumps `meta.graph_version` whenever it changes any edge row; at the end of the refresh, the hook `modules::recompute_if_stale(tx)` (called from `refresh.rs`, owned by the modules task) recomputes when `modules_version` differs. Invariant: modules after any incremental sequence equal a from-scratch run. `explain` accepts a module id.

Acceptance criteria:
- AC-11.1 Given two cliques joined by one edge in one folder, when clustered, then two modules result.
- AC-11.2 Given files of two projects, when clustered, then no module contains both.
- AC-11.3 Given the same graph twice, when clustered, then ids and members are equal.
- AC-11.4 Given an edit sequence refreshed step by step, when modules are read after each step, then they equal a from-scratch computation.

### FR-12 `stone scan`

`stone scan [--dry-run] [--prune]` writes `stone/arch/scan/<project>-<label>-<h6>.md` (`h6` is the first 6 hex of blake3 of the module's smallest file path when the draft is first created; a module whose files overlap an existing draft's by more than half reuses that draft's file, so names stay stable while members change; modules are named in module id order, and a clash adds `-2`) (`type: component`, `status: draft`, `generated: { by: 'process:stone-scan', at: <date> }`, `technology` from the language, `code` globs for the files of the module that no curated element owns, `uses` from component edges to owned neighbours) for each module with unowned files. Rules: scan keeps `.stone/scan-state.json` (path to blake3 of the text it last wrote); a draft whose current text does not hash to its stored value (any human edit of status, `code`, `uses` or text, or a missing state entry for a differing file) is curated and never written or deleted; a draft is rewritten only when its computed content differs (the date is kept from the existing file); with `--prune` drafts whose files all became owned elsewhere are deleted. Lock first, then re-read, compute and save; running twice gives the same bytes.

Acceptance criteria:
- AC-12.1 Given an unowned module of three files, when `scan` runs, then one draft appears with `code` covering them and `status: draft`.
- AC-12.2 Given the same tree, when `scan` runs again, then `0 written` and no file changes.
- AC-12.3 Given the draft changed by `stone edit --set status=stable` (its `generated` untouched), when `scan` runs, then the file is unchanged and the output says `kept arch/scan/...: edited`.
- AC-12.6 Given two modules in one folder with the same label, when `scan` runs, then two draft files with different `h6` suffixes exist, and a second run changes nothing.
- AC-12.4 Given `--dry-run`, when run, then paths and `new|changed|unchanged` print and nothing is written.
- AC-12.5 Given a draft whose files are now owned by a curated component, when `scan --prune` runs, then the draft is deleted; without `--prune` it is listed as stale.

### FR-13 Benchmark

`bench/code/<lang>/src/` is a frozen sample (open-source snapshot or synthetic, picked by the user, with licence file) and `expected_uses.yml` lists `{symbol, sites: ["path:line", ...]}` captured from the IDE's find-usages. `engine/tests/bench_code.rs` indexes each sample and asserts recall (expected sites found by `uses`, exact plus inferred) of at least 0.9 per language and prints precision and the exact share. A C# synthetic generator test asserts the 1 s per file bound (FR-4).

Acceptance criteria:
- AC-13.1 Given each sample, when the bench runs, then recall per language is at least 0.9 and the failing site list is printed otherwise.
- AC-13.2 Given a missing `expected_uses.yml`, when the bench runs, then it fails naming the language.

### FR-14 MCP `code` tool

`code {op: find|uses|deps|impact|path|explain, target, to?, depth?, exact?, budget?}` returns the same JSON as the CLI and is the seventh tool in `tools/list`, after `link`. Errors are the engine sentences.

Acceptance criteria:
- AC-14.1 Given `code {op: "uses", target: "OrderService.Place"}`, when called, then the JSON equals `stone code uses ... --json`.
- AC-14.2 Given `op: "path"` without `to`, when called, then the error is "path needs `to`".

## Non-functional requirements

- Platforms: Windows, macOS, Linux; timing asserts on Linux only. Offline. Rust 1.89.
- Local targets: Freightline full index under 60 s, one changed file under 1 s, recorded in the milestone file.
- Memory: facts are streamed per file; no whole-repo AST is kept.
- New dependencies: `tree-sitter` and four grammar crates (C#, Rust, Python, TypeScript/JavaScript), `petgraph`, `rayon`, `globset` (existing). Release binary size growth is recorded.
- A parse never panics on any input; a fuzz-style test feeds truncated files.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Grammar crates exceed MSRV 1.89 or the ABI of `tree-sitter` | Cannot build | Medium | T1 pins a compatible set first and records it |
| Names appearing or vanishing change other files' edges | Stale edges | High | Re-resolve by name in the same transaction; the rebuild-equals-incremental test |
| 90% bar missed in a language | Milestone late | Medium | Bench per language early; inferred edges count toward recall, exact share reported |
| Binary size and build time | Slow CI | Medium | Record size; only needed grammars |
| Scan overwrites curated text | Data loss | Low | Provenance and status guard, idempotency test |

## Alternatives considered

- Resolving through a compiler or LSP: rejected by ADR-11.
- Drift as an error: CI would fail on every new import before `uses` is updated; warnings with `--strict` instead.
- Louvain over the whole repo: crosses projects and folders; rejected by ADR-11 boundaries.

## Open questions

None.

## Decision log

- 2026-10-25 Drift is a warning; `--strict` fails - claude-code
- 2026-10-25 Inferred edges count toward recall and toward component edges - claude-code
- 2026-10-25 Modules are per project and first-level folder - claude-code
- 2026-10-25 Drafts live under `arch/scan/`; edit detection is a stored hash in `.stone/scan-state.json`, not `generated.by` or `status` - claude-code
- 2026-10-25 Draft file names carry a 6-hex suffix and are reused by file overlap, so they are unique and stable - claude-code
- 2026-10-25 Project files are indexed and trigger re-resolution of their project - claude-code
- 2026-10-25 The graph fill is batched and resumable; queries during it carry a "still building" note - claude-code
- 2026-10-25 Module recompute is a refresh hook keyed by `graph_version` - claude-code
- 2026-10-25 `impact`, `path` and `explain` take `--budget` and mark exactness - claude-code
- 2026-10-25 Resolver and extractor modules are declared up front (stubs in T1, registry in T4) so language tasks own separate files - claude-code
- 2026-10-25 Tasks are split to about 300 lines each, 19 in total - claude-code

## Tasks

### T1 Graph crate skeleton and fact types

Scope: `crates/graph` with the fact types, `Lang`, project-file facts and stub modules for every language, so later language tasks own separate files (FR-1 types); about 250 lines. First step: pin tree-sitter and grammar versions that build on Rust 1.89.
Owned files: `crates/graph/Cargo.toml`, `crates/graph/src/lib.rs`, `crates/graph/src/facts.rs`, `crates/graph/src/project.rs`, `crates/graph/src/{csharp,rust,ts,python}.rs` (stubs), `crates/graph/tests/project_facts.rs`, `Cargo.toml`.
Interfaces: `pub enum Lang { CSharp, Rust, TypeScript, JavaScript, Python }`; `pub fn lang_of(path: &str) -> Option<Lang>`; `pub fn extract(lang: Lang, path: &str, src: &str) -> FileFacts` (dispatches to the per-language modules); `pub fn project_facts(path: &str, src: &str) -> Option<ProjectFacts>`; `FileFacts { decls: Vec<Decl>, refs: Vec<RefSite>, imports: Vec<ImportDecl>, warnings: Vec<Warning> }`; `Decl { name, qname, kind: DeclKind, parent: Option<usize>, signature, doc, line_start: u32, line_end: u32 }`; `RefSite { name, qualifier: Option<String>, kind: RefKind, line: u32, scope: Option<usize> }`.
Tests: `csproj_cargo_package_tsconfig_pyproject_facts`, `lang_by_extension`, `truncated_project_file_never_panics`.
Finish line: `cargo test -p carvestone-graph project_facts` passes.
Depends on: M-2 done.
Risk: high (grammar crates, MSRV, binary size).

### T2 C# extraction

Scope: declarations, signatures, docs, reference sites for C# (FR-1); about 300 lines.
Owned files: `crates/graph/src/csharp.rs`, `crates/graph/tests/csharp_extract.rs`.
Interfaces: `pub(crate) fn extract_csharp(path: &str, src: &str) -> FileFacts`.
Tests: `csharp_decls_and_lines` (AC-1.1), `doc_comment_without_markers` (AC-1.2), `syntax_error_keeps_earlier_decls` (AC-1.3), `extract_twice_equal` (AC-1.4), `truncated_input_never_panics`.
Finish line: `cargo test -p carvestone-graph csharp_extract` passes.
Depends on: T1.
Risk: normal.

### T3 Index schema v3

Scope: tables and delete cascade (FR-2); about 150 lines.
Owned files: `crates/index/src/schema.rs`, `crates/index/src/lib.rs`, `crates/index/tests/schema_v3.rs`.
Interfaces: `SCHEMA_VERSION = 3` (existing constant, value changes); tables `symbols`, `refs`, `edges`, `symbols_fts`, `modules`; columns `files.lang`, `meta.graph_version`, `meta.modules_version`, `meta.code_building`.
Tests: `v2_db_rebuilds_at_v3` (AC-2.1), `file_delete_cascades` (AC-2.2), `four_threads_open_v2_db` (AC-2.3).
Finish line: `cargo test -p carvestone-index schema_v3` passes.
Depends on: none.
Risk: normal.

### T4 C# resolver: scopes, usings, inheritance, registry

Scope: the `World`, the resolver registry (all four languages as stubs) and C# namespace, using and base-list resolution (FR-3 without member calls); about 300 lines.
Owned files: `crates/graph/src/resolve.rs`, `crates/graph/src/resolve_csharp.rs`, `crates/graph/src/resolve_{rust,ts,python}.rs` (stubs returning no edges), `crates/graph/tests/resolve_csharp_scopes.rs`.
Interfaces: `pub trait Resolver { fn resolve_file(&self, world: &World, file: usize) -> Vec<Edge>; }`; `pub fn resolver_for(lang: Lang) -> &'static dyn Resolver`; `pub struct World`; `World::new(files: Vec<FileFacts>, projects: Vec<ProjectFacts>) -> World`; `Edge { src: usize, dst: Option<usize>, dst_name: String, kind: RefKind, exact: bool, line: u32 }`.
Tests: `using_resolves_exact` (AC-3.1), `partial_class_merged` (AC-3.4), `base_list_edges`, `untyped_name_falls_back_inferred`, `resolve_twice_equal`.
Finish line: `cargo test -p carvestone-graph resolve_csharp_scopes` passes.
Depends on: T2.
Risk: high.

### T5 C# resolver: member calls and ProjectReference

Scope: receiver typing and project edges (FR-3 rest); about 280 lines.
Owned files: `crates/graph/src/resolve_csharp.rs`, `crates/graph/src/csharp_types.rs`, `crates/graph/tests/resolve_csharp_members.rs`.
Interfaces: `fn receiver_type(world: &World, file: usize, site: &RefSite) -> Option<usize>` (private); project `imports` edges from `ProjectFacts` (existing).
Tests: `var_new_member_call_exact` (AC-3.2), `untyped_receiver_inferred` (AC-3.3), `project_reference_edge` (AC-3.5), `typed_parameter_and_field_exact`.
Finish line: `cargo test -p carvestone-graph resolve_csharp_members` passes.
Depends on: T4.
Risk: high (receiver typing rules).

### T6 Graph in refresh: persisted facts and batched fill

Scope: write facts, edges and project files in batches; resume; build note (FR-4 fill); about 300 lines.
Owned files: `crates/engine/src/refresh.rs`, `crates/engine/src/code_index.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/code_fill.rs`.
Interfaces: `pub(crate) fn update_code(repo: &Repo, changed: &[FileId], removed: &[FileId]) -> Result<CodeReport, Error>` (batched through `in_batches`); `RefreshReport.code_changed: usize` (field added); `World::from_rows` loader in engine.
Tests: `second_refresh_writes_nothing` (AC-4.4), `interrupted_fill_resumes_equal_rebuild` (AC-4.6), `deleted_file_leaves_no_edges`, `fill_matches_direct_resolve`.
Finish line: `cargo test -p carvestone-engine code_fill` passes.
Depends on: T3, T5.
Risk: high.

### T7 Incremental re-resolution

Scope: re-resolve dependents by name, project-file changes, rebuild-equals-incremental (FR-4 incremental); about 300 lines.
Owned files: `crates/engine/src/code_index.rs`, `crates/engine/tests/code_refresh.rs`.
Interfaces: `pub(crate) fn dependents_of(tx: &Transaction, names: &BTreeSet<String>) -> Result<Vec<FileId>, Error>`; `pub(crate) fn project_members(tx: &Transaction, project_file: FileId) -> Result<Vec<FileId>, Error>`.
Tests: `new_type_upgrades_inferred_edge` (AC-4.1), `random_edits_equal_rebuild_with_project_files` (AC-4.2), `one_file_under_one_second` (AC-4.3, Linux), `tsconfig_paths_change_reresolves` (AC-4.5). Invariants: incremental equals full rebuild; running twice changes nothing.
Finish line: `cargo test -p carvestone-engine code_refresh` passes.
Depends on: T6.
Risk: high (re-resolution correctness).

### T8 `code find`, `uses`, `deps`

Scope: SQL queries and CLI (FR-5); about 300 lines.
Owned files: `crates/engine/src/code.rs`, `crates/engine/src/lib.rs`, `crates/engine/tests/code_queries.rs`, `crates/cli/src/main.rs`, `crates/cli/tests/code.rs`.
Interfaces: `pub fn find(repo: &Repo, q: &CodeQuery) -> Result<CodeResult, Error>`, `uses`, `deps` likewise; `CodeQuery { target: String, to: Option<String>, depth: Option<usize>, kind: Option<String>, exact_only: bool, limit: usize, budget: Option<usize> }`; `CodeResult { query: String, hits: Vec<Hit>, truncated: bool }` (`Hit` existing); CLI `stone code <find|uses|deps>`.
Tests: `uses_marks_exactness` (AC-5.1), `exact_flag_filters` (AC-5.2), `ambiguous_target_lists_candidates` (AC-5.3), `file_deps` (AC-5.4), `budget_truncates` (AC-5.5).
Finish line: `cargo test -p carvestone-engine code_queries` and `cargo test -p carvestone-cli code` pass.
Depends on: T7.
Risk: normal.

### T9 `impact`, `path`, `explain`

Scope: petgraph traversal joined to components, tasks, docs; budget and exactness (FR-6); about 300 lines.
Owned files: `crates/engine/src/code_graph.rs`, `crates/engine/tests/code_impact.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn impact(repo: &Repo, q: &CodeQuery) -> Result<Impact, Error>`; `Impact { dependents: Vec<Hit>, components: Vec<String>, tasks: Vec<String>, docs: Vec<String>, truncated: bool }`; `pub fn path(repo: &Repo, q: &CodeQuery) -> Result<PathResult, Error>`; `PathResult { steps: Vec<Hit>, exact: bool, direction: String, truncated: bool }`; `pub fn explain(repo: &Repo, q: &CodeQuery) -> Result<Explain, Error>`.
Tests: `impact_depth_two` (AC-6.1), `cycle_terminates` (AC-6.2), `impact_joins_component_and_task` (AC-6.3), `no_path_message` (AC-6.4), `explain_component` (AC-6.5), `impact_budget_and_marks` (AC-6.6), `path_with_inferred_step` (AC-6.7).
Finish line: `cargo test -p carvestone-engine code_impact` passes.
Depends on: T8.
Risk: normal.

### T10 Code links and symbols in search

Scope: resolve `[[code:...]]` in check, links and backlinks, symbol search (FR-7); about 280 lines.
Owned files: `crates/engine/src/check_links.rs`, `crates/engine/src/links.rs`, `crates/engine/src/search.rs`, `crates/engine/tests/code_links.rs`.
Interfaces: `pub fn resolve_code_link(repo: &Repo, target: &str) -> CodeLink`; `CodeLink::{Symbol(String), Range { path: String, a: u32, b: u32 }, NotFound(String), Ambiguous(Vec<String>), BadRange { lines: u32 }}`; search hits of kind `symbol`.
Tests: `code_link_backlink` (AC-7.1), `missing_symbol_fails_check` (AC-7.2), `range_past_end` (AC-7.3), `symbol_in_search` (AC-7.4).
Finish line: `cargo test -p carvestone-engine code_links` passes.
Depends on: T8.
Risk: normal.

### T11 Component graph and drift

Scope: component edges, `get` section, check warnings, `--strict` (FR-8); about 280 lines.
Owned files: `crates/engine/src/arch_graph.rs`, `crates/engine/src/check.rs`, `crates/engine/src/query.rs`, `crates/cli/src/main.rs`, `crates/engine/tests/arch_drift.rs`.
Interfaces: `pub fn component_edges(repo: &Repo) -> Result<Vec<ComponentEdge>, Error>`; `ComponentEdge { from: String, to: String, edges: u32, first: (String, u32), inferred_only: bool }`; `Problem.severity` gains `Warning` (existing struct); `stone check --strict` (new flag).
Tests: `undeclared_dependency_warns` (AC-8.1), `strict_exits_1` (AC-8.2), `stale_uses_warns` (AC-8.3), `inferred_only_counts` (AC-8.4).
Finish line: `cargo test -p carvestone-engine arch_drift` passes.
Depends on: T8.
Risk: normal.

### T12 Rust extraction and module tree

Scope: Rust grammar extraction, Cargo workspace and `mod` tree (FR-9 first half); about 300 lines.
Owned files: `crates/graph/src/rust.rs`, `crates/graph/src/rust_modules.rs`, `crates/graph/tests/rust_extract.rs`.
Interfaces: `pub(crate) fn extract_rust(path: &str, src: &str) -> FileFacts`; `fn module_tree(world: &World, project: usize) -> ModuleTree` (crate-private).
Tests: `impl_methods_attach_to_type`, `mod_files_found` (`x.rs`, `x/mod.rs`), `path_attribute_marks_inferred`, `trait_impl_ref_site` (AC-9.4 facts), `truncated_input_never_panics`.
Finish line: `cargo test -p carvestone-graph rust_extract` passes.
Depends on: T1.
Risk: normal.

### T13 Rust resolver

Scope: `use` paths, re-exports, globs, path calls (FR-9 second half); about 300 lines.
Owned files: `crates/graph/src/resolve_rust.rs`, `crates/graph/tests/rust_resolve.rs`.
Interfaces: `Resolver` impl for Rust (the stub's body is replaced; the registry is unchanged).
Tests: `use_and_path_call_exact` (AC-9.1), `reexport_chain` (AC-9.2), `unknown_receiver_inferred` (AC-9.3), `trait_impl_edge` (AC-9.4).
Finish line: `cargo test -p carvestone-graph rust_resolve` passes.
Depends on: T4, T12.
Risk: high (re-exports, globs).

### T14 TypeScript/JavaScript extraction and resolver

Scope: grammar, tsconfig paths, import/export forms (FR-10 TS/JS); about 400 lines.
Owned files: `crates/graph/src/ts.rs`, `crates/graph/src/resolve_ts.rs`, `crates/graph/tests/ts_resolve.rs`.
Interfaces: `pub(crate) fn extract_ts(path: &str, src: &str, jsx: bool) -> FileFacts`; `Resolver` impl for TS/JS; `fn load_tsconfig(world: &World, project: usize) -> TsConfig` (private).
Tests: `tsconfig_paths_exact` (AC-10.1), `star_export_chain` (AC-10.2), `bare_package_import_unresolved`.
Finish line: `cargo test -p carvestone-graph ts_resolve` passes.
Depends on: T4.
Risk: high.

### T15 Python extraction and resolver

Scope: grammar, packages, import forms (FR-10 Python); about 350 lines.
Owned files: `crates/graph/src/python.rs`, `crates/graph/src/resolve_python.rs`, `crates/graph/tests/python_resolve.rs`.
Interfaces: `pub(crate) fn extract_python(path: &str, src: &str) -> FileFacts`; `Resolver` impl for Python.
Tests: `python_from_import_exact` (AC-10.3), `untyped_attribute_inferred` (AC-10.4), `relative_import_and_src_layout`, `self_method_exact`.
Finish line: `cargo test -p carvestone-graph python_resolve` passes.
Depends on: T4.
Risk: high.

### T16 Louvain modules

Scope: pure clustering in graph, storage and refresh hook in engine (FR-11); about 300 lines.
Owned files: `crates/graph/src/louvain.rs`, `crates/engine/src/modules.rs`, `crates/engine/src/refresh.rs`, `crates/graph/tests/louvain.rs`, `crates/engine/tests/modules.rs`.
Interfaces: `pub fn louvain(n: usize, edges: &[(u32, u32, f64)]) -> Vec<u32>` (deterministic); `pub fn compute_modules(repo: &Repo) -> Result<Vec<Module>, Error>`; `pub(crate) fn recompute_if_stale(tx: &Transaction) -> Result<bool, Error>` (called at the end of refresh); `Module { id: String, label: String, files: Vec<String> }`; `explain` accepts module ids (existing function).
Tests: `two_cliques_two_modules` (AC-11.1), `modules_do_not_cross_projects` (AC-11.2), `clustering_is_deterministic` (AC-11.3), `incremental_modules_equal_scratch` (AC-11.4).
Finish line: `cargo test -p carvestone-graph louvain` and `cargo test -p carvestone-engine modules` pass.
Depends on: T9.
Risk: normal.

### T17 `stone scan`

Scope: draft writer with state-hash guard and stable names (FR-12); about 300 lines.
Owned files: `crates/engine/src/scan.rs`, `crates/engine/tests/scan.rs`, `crates/cli/src/main.rs`.
Interfaces: `pub fn scan(repo: &Repo, opts: &ScanOpts) -> Result<ScanReport, Error>`; `ScanOpts { dry_run: bool, prune: bool }`; `ScanReport { entries: Vec<(String, ScanState)> }`; `ScanState { New, Changed, Unchanged, KeptEdited, Stale, Pruned }`; CLI `stone scan [--dry-run] [--prune]`; `.stone/scan-state.json`.
Tests: `unowned_module_gets_draft` (AC-12.1), `scan_twice_no_diff` (AC-12.2), `status_edit_kept` (AC-12.3), `dry_run_writes_nothing` (AC-12.4), `prune_deletes_owned_draft` (AC-12.5), `same_label_modules_get_distinct_names` (AC-12.6). Invariants: twice gives the same bytes; curated files are never written; lock first, then re-read and compute.
Finish line: `cargo test -p carvestone-engine scan` passes.
Depends on: T11, T16.
Risk: normal.

### T18 Benchmark harness

Scope: harness plus fixture wiring (FR-13); about 250 lines of Rust. The user supplies `expected_uses.yml` per sample; a scout prepares the samples and the capture steps.
Owned files: `crates/engine/tests/bench_code.rs`, `bench/code/{csharp,rust,ts,python}/src/**`, `bench/code/*/expected_uses.yml`, `.github/workflows/ci.yml`.
Interfaces: none new; reads `uses` (existing after T8).
Tests: `recall_per_language` (AC-13.1), `missing_expected_fails` (AC-13.2).
Finish line: `cargo test -p carvestone-engine bench_code` passes with four recall figures at or above 0.9.
Depends on: T8, T13, T14, T15, user-captured expectations.
Risk: high (the bar itself).

### T19 MCP `code` tool

Scope: one tool over the engine (FR-14); about 100 lines.
Owned files: `crates/mcp/src/tools.rs`, `crates/mcp/tests/code_tool.rs`, `crates/engine/src/guide/overview.md`.
Interfaces: tool handler `code`; `CodeArgs { op: String, target: String, to: Option<String>, depth: Option<usize>, exact: bool, budget: Option<usize> }`.
Tests: `code_uses_equals_cli_json` (AC-14.1), `path_without_to_errors` (AC-14.2), `tools_list_has_seven`.
Finish line: `cargo test -p carvestone-mcp code_tool` passes.
Depends on: T9.
Risk: normal.
