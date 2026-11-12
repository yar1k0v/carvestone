# Contributing with Claude Code

The repo ships its Claude Code setup. Clone it, install the tools below, and Claude Code works as the owner's sessions do.

## Install

| Tool | Version | Why |
|---|---|---|
| Claude Code | current | Runs the crew |
| Bash | Git Bash on Windows | Runs `scripts/check.sh`, `scripts/build.sh` and every command in the rules |
| Rust | from `rust-toolchain.toml` (1.89), via rustup | Builds `stone` |
| Node | 24 or newer | Runs the hooks in `.claude/hooks/` and the web UI |
| pnpm | 11.22.0 (`web/package.json`) | Builds `web/` |
| rust-analyzer | `rustup component add rust-analyzer` | Rust LSP for quick checks |
| typescript-language-server | `npm i -g typescript-language-server typescript` | TypeScript LSP for `web/` |

## First run

```bash
cargo build --bin stone
target/debug/stone check
claude
```

1. Trust the folder when Claude Code asks. This turns on the hooks and permissions in `.claude/settings.json`.
2. Install the 2 LSP plugins in `enabledPlugins` (`rust-analyzer-lsp`, `typescript-lsp`) from `claude-plugins-official`. If Claude Code does not offer them, run `/plugin`.
3. Claude Code reads `CLAUDE.md` and loads the `lead` skill. It asks you for the session goal.

All commands in the rules and skills are Bash, on every OS. PowerShell is optional: the `PowerShell(...)` allow rules and agent tools only apply on machines that have it, and other machines ignore them.

## What is shared

| Path | Holds |
|---|---|
| `CLAUDE.md` | Entry point: load `lead`, follow the crew rules |
| `.claude/rules/crew.md` | Roles, session flow, checks, commit rules |
| `.claude/rules/testing.md` | When and how to write a test |
| `.claude/skills/` | Role skills (`lead`, `developer`, `reviewer`) and their tools (`explore`, `planning`, `code`, `check`, `close`, `review`, `qa`, ...) |
| `.claude/agents/` | `developer` (Sonnet), `reviewer` (Opus), `checker` (Haiku) |
| `.claude/hooks/` | `stone-guard.mjs` blocks direct edits of `stone/`. `code-search-guard.mjs` sends symbol searches to stone or LSP |
| `.claude/settings.json` | Permissions, hooks, LSP plugins |

⚠ Never edit `stone/` by hand. Change it only through `target/debug/stone` (`new`, `edit`, `task`, ...). The hook and a deny rule block other edits.

⚠ The `commit-msg` hook, when installed, needs a `Refs: CS-n` trailer. Every commit belongs to a stone task.

## Optional: docs MCP server

Skills call `mcp__docs__search_docs` to read library docs. Without the server, agents read the library's docs page instead.

To add it, run any MCP docs server with a `search_docs` tool and register it under the name `docs`:

```bash
claude mcp add --transport http docs http://localhost:6280/mcp
```

The owner runs a local docs server on port 6280. The name `docs` matters: the tool names in `.claude/agents/` use it.

## Personal settings

Git ignores these files. Use them for preferences that are only yours.

| File | Example |
|---|---|
| `CLAUDE.local.md` | Reply style, personal rules |
| `.claude/settings.local.json` | `outputStyle`, extra allow rules |

Check before a push: the diff holds no personal names, e-mail addresses or local paths (`.claude/rules/crew.md#Session flow`).
