---
type: decision
title: No framework detection; connections are written and languages come from SCIP config
status: proposed
created: 2026-11-07
generated: { by: claude-code, at: 2026-11-07 }
---

## Context

Stone is locked to one stack at two layers:

- Languages: 5 indexers are hardcoded in `crates/engine/src/code_native.rs:77`.
- Frameworks: about 3,800 lines in `crates/graph/src/fw_*.rs` detect MediatR, Kafka, MassTransit, EF, minimal APIs and go_router.

Run-time links between services come only from the framework layer (`crates/engine/src/arch_graph.rs:253`, `:401`).

## Decision

- Remove all framework detection: the `fw_*` files, `framework.rs`, `endpoints.rs`, `code_topics.rs`, `code_tables.rs`, `code_settings.rs`, `code_patterns.rs`, and what reads them.
- Keep `dispatch.rs`. It links interface and trait calls for every language, so it is not framework code.
- Remove the screens built on it: Endpoints, Settings, External systems, Database and topics.
- People and agents write service connections in the store, each with a phrase and a protocol.
- Code checks only the build-time links it can prove: imports, shared libraries, project references.
- A run-time connection shows as written. Stone never reports it as wrong because the code does not show it.
- `stone/config.yml` can list any SCIP index file, with its language and file extensions. The user or CI runs the indexer.
- ⚠ Config never holds a command to run. Anyone can publish a repo with a config file, so a command there would run on every clone.
- For a language with no parser, the SCIP pass creates the files and symbols from the index.
- The 5 built-in languages keep working with no setup.

## Consequences

This replaces two parts of ADR-13: computed topics, and patterns in the LLD. The rest of ADR-13 stands.

- ⚠ A new project starts with an empty map until someone writes the connections.
- ⚠ Config indexers are untested end to end. A Java or Go trial must pass before the release claims them.
- `scan` and `map` stay. They reduce the empty-map cost.

Rejected:

| Option | Why not |
| --- | --- |
| Optional framework packs | The owner chose to drop the code |
| Plain HTTP matching | Route detection is per framework, so the lock returns |
| Read deployment files | New work, and those files show few call paths |
| SCIP only | Needs an indexer per language, and scip-python fails today (CS-105, CS-185) |
