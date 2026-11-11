---
name: questions
description: Lead skill. Asks the owner questions until a design, decision or feature is fully defined, then records the answers in stone. Use when work isn't fully defined.
argument-hint: "[topic or task-id]"
---

# questions (Lead)

Turn a vague request into confirmed decisions. Don't assume, and don't act before the owner confirms.

1. List the open decisions, and note which ones depend on another.
2. Look up the facts yourself first, in one batch: stone, code, docs. Ask the owner only for decisions.
3. Ask every decision that no longer depends on an open one, up to 4 per `AskUserQuestion` call. Each question stands alone, with 2–4 options. Put your pick first, marked "(Recommended)", with the evidence.
4. Repeat until no decision is open.
5. Summarize the decisions, one line each, and ask the owner to confirm.
6. Record what was confirmed, and say where:
   - hard to reverse: `new decision --title ...`, status `proposed`;
   - product behavior: the doc section that owns it;
   - work, if the owner asked for tasks: one `new task` per independent piece.

If the owner says stop, summarize what's settled and what's open, and write nothing.
