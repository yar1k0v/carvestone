#!/usr/bin/env node
// PreToolUse: agents change stone/ and .stone/ only through the stone binary or git.
// Reads and text that merely mentions the store pass; only writes are denied.
// A nudge, not a security boundary: it matches text, so a script that hides the path gets through.
import { readFileSync } from "node:fs";

const { tool_name: tool, tool_input: input = {} } = JSON.parse(readFileSync(0, "utf8"));

// A store path: a stone or .stone folder as a whole path component, with any prefix before it.
const STORE_PATH = /(^|[\s"'=(/\\])\.?stone([/\\]|$)/i;
const STORE_TOKEN = /(^|[\s"'=(/\\])\.?stone(?=[/\\"'\s)]|$)/i;
const SEGMENT_SPLIT = /;|&&|\|\||\||\r?\n/;
// A redirect target is a store path; the target may carry a ./, .\ or absolute prefix.
const WRITE_REDIRECT = />>?\s*["']?(?:[^\s"'<>|;&]*[/\\])?\.?stone(?=[/\\"'\s)]|$)/i;
// A write cmdlet or tee checks only its target, so a store path in a value or message passes.
const WRITE_CMDLET = /(?:^|[\s{(&])(out-file|set-content|add-content|tee)(?=\s)/i;
const PATH_OPTION = /^-(?:path|filepath|literalpath)(?::(.*))?$/i;
const VALUE_OPTION = /^-(value|encoding|inputobject|width|stream|filter|include|exclude|credential)$/i;
const SAFE_PROGRAM = /^(stone(\.exe)?|git)$/i;
const WRITE_PROGRAM =
  /^(cp|mv|rm|touch|mkdir|copy-item|move-item|remove-item|new-item|rename-item)$/i;
const IN_PLACE = /^(sed|perl)$/i;
const IN_PLACE_FLAG = /\s(-[a-z]*i|--in-place)/i;
const ENV_PREFIX = /^[A-Za-z_]\w*=\S*\s+/;

// True when the command assigns the variable a path that ends in stone or stone.exe.
function holdsBinary(name, command) {
  const assignment = new RegExp(
    `(?:^|[\\s;&|(])\\$?${name}\\s*=\\s*["']?(?:[^\\s"';&|]*[/\\\\])?stone(?:\\.exe)?["']?(?=[\\s;&|)]|$)`,
    "i",
  );
  return assignment.test(command);
}

// Splits on spaces; a quoted string or a parenthesised group stays one token.
function tokens(text) {
  const out = [];
  let i = 0;
  while (i < text.length) {
    if (/\s/.test(text[i])) {
      i++;
      continue;
    }
    const start = i;
    let depth = 0;
    let quote = "";
    for (; i < text.length; i++) {
      const c = text[i];
      if (quote) {
        if (c === quote) quote = "";
      } else if (c === '"' || c === "'") quote = c;
      else if (c === "(") depth++;
      else if (c === ")" && depth > 0) depth--;
      else if (/\s/.test(c) && depth === 0) break;
    }
    out.push(text.slice(start, i));
  }
  return out;
}

// The write target: the value of -Path, -FilePath or -LiteralPath, else the first positional
// argument (tee takes every positional one).
function cmdletTargets(segment) {
  const found = WRITE_CMDLET.exec(segment);
  if (!found) return [];
  const args = tokens(segment.slice(found.index + found[0].length));
  const named = args.findIndex((a) => PATH_OPTION.test(a));
  if (named >= 0) {
    const inline = PATH_OPTION.exec(args[named])[1]; // the -Path:value form
    const target = inline || args[named + 1];
    return target ? [target] : [];
  }
  const positional = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i].startsWith("-")) {
      if (VALUE_OPTION.test(args[i])) i++;
    } else positional.push(args[i]);
  }
  return found[1].toLowerCase() === "tee" ? positional : positional.slice(0, 1);
}

function programOf(segment, command) {
  let rest = segment.trim().replace(/^&\s*/, "");
  while (ENV_PREFIX.test(rest)) rest = rest.replace(ENV_PREFIX, "");
  const bare = (rest.split(/\s+/)[0] ?? "").replace(/^["']|["']$/g, "");
  const variable = /^\$\{?(\w+)\}?$/.exec(bare);
  if (variable) return holdsBinary(variable[1], command) ? "stone" : bare;
  return bare.split(/[/\\]/).pop();
}

function segmentWritesStore(segment, command) {
  const program = programOf(segment, command);
  if (SAFE_PROGRAM.test(program)) return false;
  if (WRITE_REDIRECT.test(segment)) return true;
  if (cmdletTargets(segment).some((target) => STORE_TOKEN.test(target))) return true;
  if (!STORE_TOKEN.test(segment)) return false;
  if (WRITE_PROGRAM.test(program)) return true;
  return IN_PLACE.test(program) && IN_PLACE_FLAG.test(segment);
}

const paths = [input.file_path, input.notebook_path, input.path].filter(Boolean);
const command = String(input.command ?? "");
const writesTool = tool === "Edit" || tool === "Write" || tool === "NotebookEdit";
const blocked = tool === "Bash" || tool === "PowerShell"
  ? command.split(SEGMENT_SPLIT).some((segment) => segmentWritesStore(segment, command))
  : writesTool && paths.some((p) => STORE_PATH.test(p));

if (blocked) {
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason:
        "stone/ and .stone/ are changed only through target/debug/stone.exe: get, list, search, new, edit, link, task, rename, delete. Run it with --help for the rest.",
    },
  }));
}
