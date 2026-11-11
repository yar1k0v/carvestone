import { bodyLines } from "./bodyLines";

export interface MatchLine {
  text: string;
  /** The section the line sits in, to open the doc at it. */
  anchor: string | null;
}

/** Characters of a long line kept on each side of the match. */
const AROUND = 70;
const TABLE_RULE = /^[\s|:-]+$/;

function wordsOf(query: string): string[] {
  return [...new Set(query.toLowerCase().split(/[^\p{L}\p{N}_-]+/u).filter((word) => word !== ""))];
}

/** A line of markdown as it reads on the page: no marks, a link as its label. */
export function plainLine(line: string): string {
  return line
    .replace(/^\s*(?:#{1,6}|[-*+]|\d+[.)]|>)\s+/, "")
    .replace(/\[\[(?:code:)?([^\]]+)\]\]/g, "$1")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\*\*|__|`/g, "")
    .replace(/^\s*\|\s*|\s*\|\s*$/g, "")
    .replace(/\s*\|\s*/g, " | ")
    .trim();
}

function around(text: string, word: string): string {
  if (text.length <= AROUND * 2) return text;
  const at = Math.max(0, text.toLowerCase().indexOf(word));
  const start = Math.max(0, at - AROUND);
  const end = Math.min(text.length, at + word.length + AROUND);
  return `${start > 0 ? "…" : ""}${text.slice(start, end).trim()}${end < text.length ? "…" : ""}`;
}

/** The first line of the text that holds the most words of the query. */
export function matchingLine(body: string, query: string): MatchLine | null {
  const words = wordsOf(query);
  let best: MatchLine | null = null;
  let score = 0;
  for (const line of bodyLines(body)) {
    if (TABLE_RULE.test(line.text)) continue;
    const text = plainLine(line.text);
    const lower = text.toLowerCase();
    const held = words.filter((word) => lower.includes(word));
    if (held.length <= score) continue;
    score = held.length;
    best = { text: around(text, held[0] ?? ""), anchor: line.section?.anchor ?? null };
    if (score === words.length) break;
  }
  return best;
}
