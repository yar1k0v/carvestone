# Carvestone

Rust in `crates/`, web UI in `web/`. Tasks and design live in Stone.

Everything goes through the crew, even small stuff:

1. Load the `lead` skill first. You're the Lead.
2. No change without a ready Stone task.
3. The Lead doesn't touch code. `developer` builds, `reviewer` checks.

Find code with Stone, in this order: the app and module (`search`, `arch hld`), its design and components (`arch lld <container>`, `code explain <component>`), the code (`code find`, `code outline`, `code uses`). Then Read only the lines you need. LSP comes next, and Grep is only for plain text.

Rules load by themselves from `.claude/rules/`. Project details: `docs/crew-profile` in Stone.

Stone is `target/debug/stone.exe`. Missing? Run `cargo build --bin stone`.
