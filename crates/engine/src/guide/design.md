# Designing a system from nothing to C4

Build the architecture of a repository in this order. Each step names the commands that do it.

1. Read first: `stone init` when the repo has no store, then `stone arch hld` and `stone arch gaps`.
2. Parts: `stone new system --title <name>`, then `stone new container --title <name>`, and set `parent`, `description` and `technology` with `stone edit <id> --set key=value`. A container is something that runs or stores data. A library is a component. A person or an outside system is a system with `stone edit <id> --set kind=person` or `kind=external`. A data store or message bus is a container with `kind=store`. A product with several systems is one system that is the `parent` of those systems; a system with a kind is never a parent and has no parent.
3. Connections: `stone arch connect <from> <to> --does <text> --over <protocol>`, one per direction, from the side that starts it. `--does` is a verb phrase, never `Uses`. `--over` is required when the connection leaves the process.
4. Code: `stone config add-code-root <path>`, then `stone arch map --write`, then `stone scan`. A language with no built-in parser needs `stone config add-scip-index`, then `stone code native`.
5. Docs: `stone new doc --title <title> --link <id>`. A flow through several parts stays a `C4Dynamic` mermaid fence in a doc.
6. Finish: repeat `stone arch gaps` until each gap left is one the owner accepts, then `stone check`, then read the diagrams with `stone arch c4 context`, `stone arch c4 container` and `stone arch c4 component <container>`.
