/** One labelled value from the lines of data at the top of a doc. */
export interface Fact {
  key: string;
  value: string;
}

export interface ReadableDoc {
  /** The text to read: no repeated title, no lines of data. */
  body: string;
  facts: Fact[];
}

const KEY = /^([A-Z][A-Za-z /-]{0,23}):\s+(.*)$/;
const HEADING_ONE = /^#\s+(.*?)\s*#*\s*$/;

function same(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

function skipBlank(lines: string[], from: number): number {
  let at = from;
  while (at < lines.length && (lines[at] ?? "").trim() === "") at += 1;
  return at;
}

function paragraphEnd(lines: string[], from: number): number {
  let at = from;
  while (at < lines.length && (lines[at] ?? "").trim() !== "") at += 1;
  return at;
}

/** `A: 1 · B: 2` as facts; a piece with no key of its own belongs to the fact before it. */
function factsOf(line: string): Fact[] {
  const out: Fact[] = [];
  for (const piece of line.split(" · ")) {
    const match = KEY.exec(piece.trim());
    const last = out[out.length - 1];
    if (match?.[1] !== undefined && match[2] !== undefined) out.push({ key: match[1], value: match[2].trim() });
    else if (last !== undefined) last.value = `${last.value} · ${piece.trim()}`;
  }
  return out;
}

/** Joins a line that wrapped to the line it continues: only a line with a key starts a new one. */
function unwrap(lines: string[]): string[] {
  const out: string[] = [];
  for (const line of lines.map((text) => text.trim())) {
    const last = out[out.length - 1];
    if (last === undefined || KEY.test(line)) out.push(line);
    else out[out.length - 1] = `${last} ${line}`;
  }
  return out;
}

/** The lines are data when each starts with a key and at least one packs several with ` · `. */
function isFacts(lines: string[]): boolean {
  return lines.length > 0 && lines.every((line) => KEY.test(line)) && lines.some((line) => line.includes(" · "));
}

/** The body without a first heading that repeats the title. */
export function dropTitle(body: string, title: string): string {
  const lines = body.split(/\r?\n/);
  const first = skipBlank(lines, 0);
  const heading = HEADING_ONE.exec(lines[first] ?? "");
  if (heading?.[1] === undefined || !same(heading[1], title)) return body;
  return lines.slice(skipBlank(lines, first + 1)).join("\n");
}

/** Splits a doc into what is read and the data its first lines hold. */
export function readDoc(body: string, title: string): ReadableDoc {
  const lines = dropTitle(body, title).split(/\r?\n/);
  const facts: Fact[] = [];
  let at = skipBlank(lines, 0);
  for (;;) {
    const end = paragraphEnd(lines, at);
    const paragraph = unwrap(lines.slice(at, end));
    if (!isFacts(paragraph)) break;
    facts.push(...paragraph.flatMap(factsOf));
    at = skipBlank(lines, end);
  }
  return { body: lines.slice(at).join("\n"), facts };
}

export type FactPart = { kind: "link"; target: string } | { kind: "code"; text: string };

export interface ParsedFact {
  parts: FactPart[];
  /** What the value says besides its links and code. */
  rest: string;
}

/** The links and code spans of a value, and the words around them. */
export function parseFact(value: string): ParsedFact {
  const parts: FactPart[] = [];
  const rest = value.replace(/\[\[([^\]]+)\]\]|`([^`]+)`/g, (_, target?: string, code?: string) => {
    if (target !== undefined) parts.push({ kind: "link", target: target.trim() });
    else if (code !== undefined) parts.push({ kind: "code", text: code.trim() });
    return " ";
  });
  return { parts, rest: rest.replace(/[\s,;]+/g, " ").trim() };
}

/** `a/b/c/d.dart#Name` reads `Name`; `a/b/c/d` reads `…/c/d`. */
export function shortCode(code: string): string {
  const [path = code, symbol] = code.split("#");
  if (symbol !== undefined && symbol !== "") return symbol;
  const segments = path.split("/").filter(Boolean);
  return segments.length <= 2 ? path : `…/${segments.slice(-2).join("/")}`;
}

/** Where a code span opens: its symbol, else its path. */
export function codeTarget(code: string): string {
  const [path = code, symbol] = code.split("#");
  return symbol !== undefined && symbol !== "" ? symbol : path;
}
