---
type: doc
title: Docs
status: stable
created: 2026-11-11
generated: { by: claude-code, at: 2026-11-11 }
description: Project docs as markdown files, linked to code and served as a site.
components: [arch/carvestone/stone/engine, arch/carvestone/stone/server, arch/carvestone/stone/store, arch/carvestone/web-ui/web]
links: [ADR-9, ADR-14, ADR-16]
---

Stone keeps project docs as markdown files in `stone/docs/`, and serves them as a site.

Status: live

## What it does

- A doc is one markdown file. The folders under `stone/docs/` become the site sidebar.
- `stone serve` serves the site at `http://127.0.0.1:4747` with backlinks and Mermaid, and watches files for changes.
- Each heading is a section with an anchor. `stone get <id>#<anchor>` prints one section.
- A doc links with markdown links, `[[CS-12]]`, `[[docs/a#section]]`, and code links such as `[[code:path/file.rs:120-160]]`. The index computes backlinks for all of them.
- A doc names the components it describes in `components`. A doc marked `level: hld` or `level: lld` shows in `stone arch hld` and `stone arch lld`.
- The Product page reads `docs/product-overview` and every `docs/feature-*` doc. A `Status:` line above the first section sets the status of a feature.
- Library docs stay in Docs MCP. `reference` items and component `libraries` point at it ([[ADR-9]]).

## Commands

| Command | Purpose |
| --- | --- |
| `stone new doc --slug <slug> --title "<title>"` | Create a doc |
| `stone new doc --from <file>` | Create a doc from a markdown file, keeping its text and links |
| `stone edit <id> --section <name>` | Replace one section |
| `stone get <id>#<anchor>` | Print one section |
| `stone list --type doc` | List docs |
| `stone list --stale` | List docs, references and architecture items that need another look |
| `stone rename <id> <new-id>` | Give a doc a new id and update every link to it |
| `stone delete <id>` | Delete a doc. Refused while another item links to it |
| `stone serve` | Serve the site |
| `stone check` | Validate the store |

## Limits

- ⚠ `stone edit <id> --section <top heading>` on a level-1 heading replaces the whole body (known defect, [[CS-228]]). Use `--set` for fields.
- Library docs are not stored or served here. Docs MCP does that.
- No file store for images, decks and PDFs.
- The web UI is read-only for docs ([[ADR-14]]). Text edits go through the CLI.

## Planned

- [[CS-236]]: `index.md` lists every item with its description.
- [[CS-237]]: `stone export --okf` writes relations as standard markdown links.
- [[CS-228]]: `edit --section` warns before it drops nested sections.
- [[CS-188]]: `stale_after` accepts only a date.
