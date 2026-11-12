# Crew rules

## Roles

1. Roles are skills: `lead`, `developer`, `reviewer`. Any agent takes a role by loading its skill. The main session included.
2. Only the Lead talks to the owner and hands out tasks. Report as to a product owner.
3. NEVER change a file without a ready Stone task (`task check <id>` prints `is ready`).
4. Checks run in the `checker` agent (Haiku). The main session runs only `cargo fmt --all` and `cargo build --bin stone`. NEVER run another cargo or pnpm command there.

## Session flow

1. At session start, ask the owner for the session goal with `AskUserQuestion`. The tasks of that goal are the batch.
2. Plan every task of the batch to ready before the first build.
3. Give related tasks (same component or same files) to one developer run, in one hand-off that lists every id.
4. The developer runs only each task's acceptance commands, then `close`.
5. Run one review, and QA if the owner said yes once, when every task of the goal is built. NEVER review or QA per task unless the owner asks.
6. For QA, the Lead saves the old CLI output of each flow in `docs/agent-scratch/qa-before/` before the build. NEVER build a second worktree.
7. Run the full checks once, after the review.
8. A finding goes back to its own task only.
9. Commit once per task: `type(scope): summary`, then a blank line and `Refs: CS-n`. NEVER commit or push without the owner's approval.
10. Before a push, search the unpushed diff for personal names, e-mail addresses and local paths.

## Talking to the owner

1. Reply BLUF, at most 5 lines.
2. Ask every question with `AskUserQuestion`, the recommended option first.
3. Ask only for a real decision: scope, a product rule, a commit, a push. NEVER ask the run mode or a per-task QA question.

## Working

1. Plan requests first: each one re-sends the whole context. Send independent calls together.
2. Find code in this order: stone (`brief`, `code find`, `outline`, `uses`, `deps`, `impact --depth 2`, `why <path>:<line>`), LSP, then Grep for plain text.
3. Quick checks: LSP diagnostics and stone. If LSP is not enough, `checker` runs `cargo check -p <crate>`, or `pnpm --dir web typecheck` for `web/`. NEVER run clippy or a workspace build per task.
4. NEVER fetch again what `explore` loaded. Read with offset and limit. NEVER re-read a file you edited.
5. Edit files only with Edit and Write.
6. On a `note:` line from stone, run `target/debug/stone.exe code native` once.
7. A wrong or empty stone answer is a product finding: one line under `## Notes`, then go on.
8. Before writing library code, call `search_docs` with the version in the component's `libraries`.
9. Load deferred tools (LSP, `search_docs`) with `ToolSearch` first.
10. Start shell commands with the program. NEVER use `cd ... &&`, `git -C` or a pipe into `head`.
11. Use Glob, Grep and Read. NEVER use `find`, `ls`, `cat`, `grep` or `tail` in the shell.
12. NEVER use external agents unless the owner asks.
13. Write store items only through `target/debug/stone.exe` (`edit --note` for notes): it takes the lock. NEVER edit `stone/` with Edit, Write or sed.

## Code

1. Every store write takes the lock, then re-reads, computes and saves.
2. Writes are minimal-diff: only touched lines change, CRLF stays CRLF, the same write twice gives no diff.
3. `✗` messages belong to the CLI. Engine errors are plain sentences that name the next step.
4. Parse body links, fences and code spans only with the scanners in `crates/core/src/section.rs`.
5. Comments say why. NEVER cite plans, ids or dates.

## Checks

1. One cargo process at a time: `bash scripts/check.sh <label> <command>`. It saves the output in `docs/agent-scratch/<label>.txt` and prints the result lines. Read the file only when those lines are not enough.
2. One topic: `cargo test -p <crate> --test features <topic>::`. ⚠ A filter can match zero tests and pass: read the `N passed` line.
3. The same check fails twice: stop and report.
4. Run `pnpm --dir web build` before `cargo build --features ui` (it embeds `web/dist`).
5. Perf and timing tests: release build, nothing else compiling.
6. After a code change, run `cargo build --bin stone` before using `target/debug/stone.exe`.
7. Full checks, once at the end. Add the `pnpm` line when `web/` changed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
target/debug/stone.exe check
pnpm --dir web typecheck && pnpm --dir web lint && pnpm --dir web test && pnpm --dir web build
```
