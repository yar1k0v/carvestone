# Crew rules

Roles: the Lead (main session) decides and plans. The `developer` agent builds one ready task. The `reviewer` agent accepts or returns it. Only the Lead hands out tasks or talks to the owner, and reports as to a product owner.

The Lead may accept a tiny docs fix itself, as the lead skill defines it, while its trial runs.

## Working

- Every request re-sends the whole context. Plan the requests first, and send independent calls in one message.
- Don't fetch what explore already loaded. Read with offset and limit, and never re-read a file after editing it.
- Find code in this order: stone (`code find`, `outline`, `uses`, `deps`, `impact --depth 2`, `why <path>:<line>`), then LSP, then Grep for text stone doesn't hold. A `note:` line on an answer: run `code native` once.
- A wrong or empty stone answer is a product finding: one line under `## Notes`, then go on.
- Library behavior: docs MCP `search_docs` before writing the code, with the version from the component's `libraries`.
- LSP and `search_docs` are deferred: load them with `ToolSearch` first.
- Shell commands start with the program itself: no `cd ... &&`, no `git -C`, no pipes into `head`. Use Glob, Grep and Read, never `find`, `ls`, `cat`, `grep` or `tail`.
- Edit only with Edit and Write. No commits unless the owner asks. External agents only when the owner asks.
- Comments say why, and never cite plans, ids or dates.

## Code

- Every store write takes the lock first, then re-reads, computes and saves.
- Writes are minimal-diff: only the touched lines change, CRLF stays CRLF, and the same write twice gives no diff.
- `✗` messages belong to the CLI only. Engine errors are plain sentences that name the next step.
- Body links, fences and code spans go through the shared scanners in `crates/core/src/section.rs`. Never write a second parser.

## Checks

- `cargo build --features ui` embeds `web/dist`, so run `pnpm --dir web build` first.
- One cargo process at a time. Redirect its output to a file in `docs/agent-scratch/` and Read the file.
- The same check failing twice: stop and report.
- Quick: rust-analyzer diagnostics in the main session, `cargo check -p <crate>` in a subagent, `pnpm --dir web typecheck` for `web/`.
- One topic: `cargo test -p <crate> --test features <topic>::`. ⚠ A filter can match zero tests and still pass, so check the `N passed` line.
- Perf and timing tests: release build, nothing else compiling.
- After a code change: `cargo build --bin stone`, then use `target/debug/stone.exe`.

Full checks, once at the end:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
target/debug/stone.exe check
```

When `web/` changed, also:

```bash
pnpm --dir web typecheck
pnpm --dir web lint
pnpm --dir web test
pnpm --dir web build
```
