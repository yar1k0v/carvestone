#!/usr/bin/env node
// PreToolUse on Grep: a symbol lookup in code goes to stone or LSP, which know definitions and callers.
// A nudge, not a boundary: plain words, phrases and regexes still pass, because Grep is right for text.
import { readFileSync } from "node:fs";

const { tool_input: input = {} } = JSON.parse(readFileSync(0, "utf8"));
const pattern = String(input.pattern ?? "").trim();
const path = String(input.path ?? "").replaceAll("\\", "/");
const filter = `${input.glob ?? ""} ${input.type ?? ""}`.trim();

const IDENTIFIER = /^(?:\w+::)*(?:[a-z][a-z0-9]*_[a-z0-9_]+|[A-Z][a-z0-9]+[A-Z]\w*|[a-z]+[A-Z]\w*)$/;
const DECLARATION = /^(?:pub\s+)?(?:fn|struct|enum|trait|impl|class|function|interface|type|const)\s+\w+$/;
const CODE_PATH = /(^|\/)(crates|web\/src)(\/|$)/;
const REPO_ROOT = /^(\.?|.*\/carvestone\/?)$/i;
const CODE_FILTER = /rs|rust|ts|tsx/;

const looksLikeSymbol = IDENTIFIER.test(pattern) || DECLARATION.test(pattern);
const inCode = CODE_PATH.test(path) || REPO_ROOT.test(path);
const codeFiles = !filter || CODE_FILTER.test(filter);

if (looksLikeSymbol && inCode && codeFiles) {
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: `"${pattern}" looks like a symbol. Use target/debug/stone code find, code uses or code outline, or the LSP tool. Grep is for plain text.`,
    },
  }));
}
