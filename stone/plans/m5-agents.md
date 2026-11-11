---
type: plan
title: M-5 agents requirements
description: The stone MCP server, shared workflow guides and library hints, as checkable behaviour. Serves the M-5 task list.
status: done
milestone: M-5
links: [M-5, ADR-2, ADR-7, ADR-9, ADR-10, docs/prd, plans/v1-roadmap]
created: 2026-10-25
generated: { by: 'claude-code', at: 2026-10-25 }
---

## Summary

At the end of M-5 an MCP client (Claude Code) starts `stone mcp`, lists six tools, and runs a task through them: search, get a section, create, update, link. The workflow guides (overview, plan, do, finish) come from one text in the engine, printed by `stone instructions` and served as MCP resources. Components with `libraries` point the agent at Docs MCP `search_docs`. The `code`, `why` and `history` tools arrive with M-3 and M-4. Server, UI and importers are out of scope.

## Background

- ADR-10: cli, mcp and server call engine only; adapters parse input and render output. MCP is thin; if a tool needs logic, it belongs in the engine.
- M-2 gave the engine `search`, `get` with backlinks, `list`, `refresh`; M-1 gave `create`, `edit`, `task move`, `link`. `crates/cli/src/instructions.md` holds the M-1 guide as a CLI-only string.
- Roadmap: tools search, get, list, create, update, link; `stone mcp` over stdio with `rmcp`; the user registers it in `.mcp.json`, Claude does not edit that file; risk is rmcp's MSRV against Rust 1.89.
- Errors in the engine are plain sentences; MCP returns the sentence as a tool error (ADR-10, roadmap).

## Goals

- Claude Code finishes a stone task with MCP alone, never shelling to `stone`.
- Tool results are small: the same JSON structs as `--json`, compact, with the same budgets.
- The guide is written once; CLI and MCP cannot drift.

## Users and scenarios

- An agent calls `search {query: "jwt refresh"}`, then `get {id: "docs/auth", section: "tokens"}`, edits via `update`, finishes with `update {id, set: {status: done}}`. Failure path: moving to done without an `## Outcome` returns the M-1 sentence as a tool error, and the file is unchanged.
- An agent creates a task: `create {type: "task", title, fields: {priority: "high"}}` and gets the new id. Failure path: `fields: {status: "bogus"}` returns the allowed statuses and writes nothing.
- A person registers the server and opens the client; `tools/list` and `resources/list` show what the guide says. Failure path: the client starts `stone mcp` outside a stone repo; it exits 1 with `no stone/ found; run \`stone init\`` on stderr, stdout stays empty.
- The agent reads a component with `libraries: [mediatr@12]` and sees the `search_docs` hint instead of guessing from memory.

## Scope

- In: crate `crates/mcp`, `stone mcp`, six tools, four guide resources, one shared guide text, library hints in `instructions` and `get`, E2E over stdio, a README snippet for the user's `.mcp.json`.
- Out: `code`, `why`, `history` tools (M-3, M-4), HTTP (M-6), prompts, sampling, remote transports, authentication.

## Functional requirements

### FR-1 Server and handshake

`stone mcp [--root PATH]` serves MCP over stdio using `rmcp`. It finds the repo root like every command, exits 1 before serving when none is found, writes only protocol messages to stdout and diagnostics to stderr, and ends cleanly when stdin closes. `initialize` answers with name `stone`, the crate version, and capabilities for tools and resources. `instructions` in the result is the overview guide's first lines (under 25) ending with the Docs MCP pointer of FR-5. `tools/list` returns, in this order: `search`, `get`, `list`, `create`, `update`, `link`, each with a JSON input schema and a one-line description. The author of writes is `STONE_AGENT` when set, otherwise `<clientInfo.name>/<clientInfo.version>` lowercased with spaces replaced by `-` (so it is a valid actor, M-1 AC-2.6), otherwise `process:mcp`.

Acceptance criteria:
- AC-1.1 Given a repo, when a client sends `initialize` then `tools/list`, then the server name is `stone` and the tool names equal the list above.
- AC-1.2 Given no `stone/config.yml`, when `stone mcp` starts, then it exits 1, stderr holds `✗ no stone/ found; run \`stone init\``, stdout is empty.
- AC-1.3 Given a running server, when stdin closes, then the process exits 0 within 2 s.
- AC-1.4 Given any call, when it runs, then stdout holds only newline-delimited JSON-RPC messages.
- AC-1.5 Given client `Claude Code` 2.1 and `STONE_AGENT` unset, when `create` runs, then `generated.by` is `claude-code/2.1` and `stone check` accepts it.
- AC-1.6 Given `initialize`, when answered, then `instructions` has at most 25 lines and its last line names Docs MCP `search_docs`.

### FR-2 Read tools

- `search {query, from?, component?, type?, status?, milestone?, limit?, budget?}` returns `SearchResult` as compact JSON. `budget` defaults to 400 here.
- `get {id, section?}` returns the `GetResult` JSON: with `section`, only that section's text and range; always with backlinks (M-2). A text field names the file path.
- `list {type?, status?, milestone?, component?, ready?}` returns the rows; `ready: true` with `type: task` gives the `task next` result (todo tasks whose dependencies are done), default limit 5.
All three refresh the index first (M-2) and never write under `stone/`.

Acceptance criteria:
- AC-2.1 Given this repo, when `search {query: "minimal diff", budget: 200}` runs, then hits have `id, anchor, path, lines, kind, snippet` and their summed cost is at most 200.
- AC-2.2 Given `get {id: "CS-2", section: "nope"}`, when called, then the result is an error listing CS-2's anchors.
- AC-2.3 Given `list {type: "task", ready: true}` with CS-3 depending on a todo CS-2, when called, then CS-2 is listed and CS-3 is not.
- AC-2.4 Given the same arguments, when `search` runs through the CLI `--json` and through MCP, then the JSON values are equal.

### FR-3 Write tools

- `create {type, title, slug?, fields?, sections?}`: `fields` is a map of key to string (list fields take `a,b`); `sections` maps a section name to text; returns `{id, path}`.
- `update {id, set?, unset?, section?: {name, text}, check_ac?, dry_run?}`: the engine edit path; a `status` set on a task goes through `task move` rules (a done task needs `## Outcome`); returns `{id, changed: bool, text?}`, where `text` is the resulting file for `dry_run`.
- `link {from, to: [ids]}`: returns `{id, added: [...]}`.
All three call the same engine functions as the CLI, take the store lock, and give a second identical call `changed: false` with no file change.

Acceptance criteria:
- AC-3.1 Given `create {type: "task", title: "X", fields: {priority: "high"}}`, when called, then the result id is the next task id and the file has `priority: high`, `status: todo`.
- AC-3.2 Given the same `update` twice, when called, then the second result is `changed: false` and the file is byte-identical.
- AC-3.3 Given `update {id: "CS-2", set: {status: "done"}}` on a task without `## Outcome`, when called, then the tool error is "CS-2 has no ## Outcome; add one with `stone edit CS-2 --section outcome`" and the file is unchanged.
- AC-3.4 Given `link {from: "CS-2", to: ["ADR-3", "docs/nope"]}`, when called, then the error says `docs/nope not found` and CS-2 is unchanged.
- AC-3.5 Given 4 parallel `create` calls over four server processes, when all return, then the ids are distinct.

### FR-4 Workflow guides

`engine::guide` holds four markdown texts: overview, plan, do, finish. `guide(Topic) -> &'static str` returns them. The overview names only commands and tools that exist; each guide stays under 40 lines. `stone instructions [overview|plan|do|finish]` prints one (default overview). The MCP server lists resources `stone://guide/overview`, `stone://guide/plan`, `stone://guide/do`, `stone://guide/finish` (mime `text/markdown`) and `resources/read` returns the same text byte for byte. The guides are written for tools; where the CLI and MCP name differ they show both (`stone task next` / `list {type: task, ready: true}`).

Acceptance criteria:
- AC-4.1 Given the server, when `resources/list` runs, then it returns the four URIs with titles.
- AC-4.2 Given each topic, when `resources/read` and `stone instructions <topic>` run, then both texts are equal.
- AC-4.3 Given the overview, when read, then every command name in backticks is a clap subcommand or an MCP tool that exists (a test parses both).
- AC-4.4 Given `stone instructions` with no topic, when it runs, then it prints the overview in 25 lines or fewer.
- AC-4.5 Given `resources/read` for `stone://guide/nope`, when called, then the error is "unknown guide nope; use overview, plan, do or finish".

### FR-5 Library hints

For system, container and component items with `libraries`, `stone get` text prints after the body `Libraries (look up in Docs MCP with search_docs): mediatr@12, fluentvalidation@11`; `--json` and the MCP `get` add `libraries: [{library, version}]` (version null when absent). The overview guide ends with: "Library docs are not in stone. Use Docs MCP `search_docs` with the library and version listed under a component's `libraries`." `reference` items with `library` get the same line.

Acceptance criteria:
- AC-5.1 Given a component with `libraries: [mediatr@12, serilog]`, when `get --json` runs, then `libraries` is `[{library: "mediatr", version: "12"}, {library: "serilog", version: null}]`.
- AC-5.2 Given a component without `libraries`, when `get` runs, then no `Libraries` line prints.
- AC-5.3 Given the overview, when read, then it names `search_docs`.

### FR-6 Error mapping

An engine error becomes a tool result with `isError: true` and one text content item holding the sentence (no `✗`, no stack, no JSON). Bad arguments (missing `id`, unknown `type`) fail at schema validation with a message naming the field. A panic in a tool handler returns "internal error; run stone check and retry" and keeps the server alive.

Acceptance criteria:
- AC-6.1 Given `get {id: "CS-999"}`, when called, then `isError` is true and the text starts `CS-999 not found`.
- AC-6.2 Given `get {}`, when called, then the error names `id`.
- AC-6.3 Given a handler that panics (test hook), when called, then the next call succeeds.

### FR-7 End to end

A test spawns the built `stone mcp` against a temp repo and speaks JSON-RPC over its pipes: initialize, `tools/list`, `search`, `create`, `update` to done after adding an outcome, `get`; then reads a resource. A second test drives the server in-process over a duplex stream for the tool matrix.

Acceptance criteria:
- AC-7.1 Given the E2E run, when it ends, then every response matches its request id and no call timed out (5 s each).
- AC-7.2 Given the final file, when read, then it equals what the CLI would have written for the same steps.

## Non-functional requirements

- Platforms: Windows, macOS, Linux; stdio uses binary-safe newline framing, LF only.
- Startup under 300 ms on this repo; a read tool under 100 ms warm.
- Offline. Rust 1.89. New dependencies: `rmcp` (server, stdio transport features only), `tokio` (rt, io), `schemars` as rmcp requires. The workspace builds with no network after `cargo fetch`.
- A tool call never reads more than the engine returns; no tool returns a whole file unless `get` without `section` is asked.

## Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| rmcp MSRV above 1.89 | Cannot build | Medium | T1 starts by pinning a version that builds on 1.89; if none does, a hand-written JSON-RPC stdio loop (initialize, tools, resources only) behind the same engine calls |
| Stray stdout writes corrupt the stream | Client hangs | Low | AC-1.4 test; `clippy::print_stdout` denied in `crates/mcp` |
| Spec drift across rmcp releases | Client mismatch | Medium | Pin exact version; E2E on every CI run |
| Concurrent MCP and CLI writes | Lost update | Low | Same engine lock path (M-1 invariant) |

## Alternatives considered

- `structuredContent` plus text: doubles tokens for the same data; one compact JSON text is used.
- A `task` tool for moves: `update` with status keeps six tools and the same Outcome rule.
- Guides as MCP prompts: the roadmap and PRD say resources.

## Open questions

None.

## Decision log

- 2026-10-25 `list` takes `ready` so `task next` needs no seventh tool - claude-code
- 2026-10-25 Results are compact JSON text, equal to `--json` - claude-code
- 2026-10-25 MCP writes are attributed to the client's name and version (lowercased, spaces as `-`) unless `STONE_AGENT` is set - claude-code
- 2026-10-25 `search` returns the shared `{query, hits, truncated}` shape, equal to `stone search --json` - claude-code
- 2026-10-25 Tasks are split to one concern each (server, read tools, write tools, guide, hints, E2E) - claude-code
- 2026-10-25 The spawn E2E lives in the cli crate because only that package can use `CARGO_BIN_EXE_stone`; the mcp crate keeps the in-process matrix - claude-code

## Tasks

### T1 MCP crate, stdio server and error mapping

Scope: `crates/mcp` with rmcp stdio, `initialize`, empty tool router, error mapping, `stone mcp` (FR-1, FR-6); about 250 lines. First step: record the rmcp version that builds on Rust 1.89.
Owned files: `crates/mcp/Cargo.toml`, `crates/mcp/src/lib.rs`, `crates/mcp/src/errors.rs`, `crates/mcp/tests/server.rs`, `Cargo.toml` (workspace member), `crates/cli/src/main.rs`, `crates/cli/Cargo.toml`.
Interfaces: `pub async fn serve_stdio(repo: Repo) -> Result<(), McpError>`; `pub fn server(repo: Repo) -> StoneServer`; `pub struct StoneServer`; `McpError` wraps engine and transport errors; `fn tool_error(e: &engine::Error) -> CallToolResult`; CLI `stone mcp [--root]` (new).
Tests: `no_repo_exits_1_stdout_empty` (AC-1.2), `stdin_close_exits_0` (AC-1.3), `stdout_only_json_rpc` (AC-1.4), `client_name_is_author` (AC-1.5), `unknown_id_is_tool_error` (AC-6.1), `missing_id_names_field` (AC-6.2), `handler_panic_keeps_server_alive` (AC-6.3).
Finish line: `cargo test -p carvestone-mcp server` passes.
Depends on: M-2 done.
Risk: high (dependency MSRV and API shape).

### T2 Read tools

Scope: `search`, `get`, `list` handlers and the tool list order (FR-1 tools/list, FR-2); about 250 lines.
Owned files: `crates/mcp/src/tools.rs`, `crates/mcp/src/lib.rs`, `crates/mcp/tests/read_tools.rs`.
Interfaces: handlers `search`, `get`, `list`; `SearchArgs { query, from, component, r#type, status, milestone, limit, budget }`; `GetArgs { id, section }`; `ListArgs { r#type, status, milestone, component, ready }`.
Tests: `lists_read_tools_in_order` (AC-1.1), `search_matches_cli_json` (AC-2.4), `get_bad_section_lists_anchors` (AC-2.2), `list_ready_excludes_blocked` (AC-2.3), `search_budget_cost` (AC-2.1). Invariant: read tools write nothing under `stone/`.
Finish line: `cargo test -p carvestone-mcp read_tools` passes.
Depends on: T1.
Risk: normal.

### T3 Write tools

Scope: `create`, `update`, `link`, author attribution (FR-3); about 250 lines.
Owned files: `crates/mcp/src/tools.rs`, `crates/mcp/tests/write_tools.rs`, `crates/engine/src/repo.rs`.
Interfaces: handlers `create`, `update`, `link`; `CreateArgs { r#type: String, title: String, slug: Option<String>, fields: BTreeMap<String, String>, sections: BTreeMap<String, String> }`; `UpdateArgs { id: String, set: BTreeMap<String, String>, unset: Vec<String>, section: Option<SectionArg>, check_ac: Option<u32>, dry_run: bool }`; `Repo::with_author(self, actor: String) -> Repo` (new).
Tests: `create_task_gets_next_id` (AC-3.1), `update_twice_unchanged` (AC-3.2), `done_without_outcome_refused` (AC-3.3), `link_bad_target_atomic` (AC-3.4). Invariants: running any write twice gives the same file; each write takes the lock, then re-reads, computes and saves.
Finish line: `cargo test -p carvestone-mcp write_tools` passes.
Depends on: T2.
Risk: normal.

### T4 Shared guide and resources

Scope: guide texts in the engine, `stone instructions [topic]`, MCP resources, `initialize` instructions (FR-1 instructions, FR-4); about 250 lines, mostly text.
Owned files: `crates/engine/src/guide.rs`, `crates/engine/src/lib.rs`, `crates/engine/src/guide/overview.md`, `crates/engine/src/guide/plan.md`, `crates/engine/src/guide/do.md`, `crates/engine/src/guide/finish.md`, `crates/cli/src/main.rs`, `crates/cli/src/instructions.md` (deleted), `crates/mcp/src/resources.rs`, `crates/mcp/src/lib.rs`, `crates/engine/tests/guide.rs`, `crates/mcp/tests/resources.rs`.
Interfaces: `pub enum Topic { Overview, Plan, Do, Finish }`; `pub fn guide(topic: Topic) -> &'static str`; `Topic::from_name(&str) -> Option<Topic>`; CLI `stone instructions [TOPIC]` (existing, gains the argument); MCP `resources/list`, `resources/read`.
Tests: `resources_list_four_uris` (AC-4.1), `resource_equals_cli_text` (AC-4.2), `overview_names_only_real_commands` (AC-4.3), `overview_under_25_lines` (AC-4.4), `unknown_guide_message` (AC-4.5), `initialize_instructions_under_25_lines_and_name_search_docs` (AC-1.6).
Finish line: `cargo test -p carvestone-engine guide` and `cargo test -p carvestone-mcp resources` pass.
Depends on: T2.
Risk: normal.

### T5 Library hints

Scope: `libraries` split and hint in `get` (FR-5); about 120 lines.
Owned files: `crates/engine/src/query.rs`, `crates/engine/src/guide/overview.md`, `crates/cli/src/main.rs`, `crates/engine/tests/libraries.rs`.
Interfaces: `GetResult.libraries: Vec<LibraryRef>` (field added to the existing struct); `LibraryRef { library: String, version: Option<String> }`; `pub fn parse_library(s: &str) -> LibraryRef`.
Tests: `libraries_split_name_and_version` (AC-5.1), `no_libraries_no_line` (AC-5.2), `overview_names_search_docs` (AC-5.3).
Finish line: `cargo test -p carvestone-engine libraries` passes.
Depends on: T4.
Risk: normal.

### T6 End-to-end tests and README snippet

Scope: spawn test and in-process matrix (FR-7); about 250 lines.
Owned files: `crates/cli/tests/mcp_e2e.rs`, `crates/mcp/tests/matrix.rs`, `README.md` (MCP section with the `.mcp.json` snippet the user adds).
Interfaces: none new; test helper `fn rpc(child: &mut Child, id: u64, method: &str, params: Value) -> Value` (test only).
Tests: `spawned_server_full_task_flow` (AC-7.1), `result_equals_cli_flow` (AC-7.2), `four_servers_create_distinct_ids` (AC-3.5).
Finish line: `cargo test -p carvestone-cli mcp_e2e` and `cargo test -p carvestone-mcp matrix` pass.
Depends on: T3, T4, T5.
Risk: normal.
