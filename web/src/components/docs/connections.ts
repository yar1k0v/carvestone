import type { Backlink } from "@/api/types/Backlink";
import type { ListRow } from "@/api/types/ListRow";
import { serviceHref } from "@/components/graph/serviceHref";
import { type Resolver, codeHref, hrefFor } from "@/components/markdown/links";
import { bodyLines } from "./bodyLines";
import { type Fact, codeTarget, parseFact, shortCode } from "./lead";

/** One thing a doc connects to, and how. */
export interface Connection {
  key: string;
  title: string;
  /** What the title shortens, shown on hover. */
  hint?: string;
  /** A task's short id. */
  short?: string;
  status?: string;
  href: string;
  relation: string;
}

export interface Connections {
  documents: Connection[];
  tasks: Connection[];
  architecture: Connection[];
}

export interface ConnectionSource {
  self: string;
  facts: Fact[];
  /** The doc's own `links` and `components` fields. */
  links: string[];
  /** The text as it reads, without the data lines the facts hold. */
  body: string;
  backlinks: Backlink[];
  rows: ListRow[];
  resolver: Resolver;
}

type Group = keyof Connections;

const ARCH_TYPES = new Set(["system", "container", "component"]);
const WORK_TYPES = new Set(["task", "plan", "milestone"]);

const FACT_RELATIONS: Record<string, string> = {
  "Arch item": "The part this document describes",
  Related: "Listed as related",
};

const CODE_RELATIONS: Record<string, string> = {
  Code: "Code this document covers",
  Entry: "Where the code starts",
};

const WIKILINK = /\[\[([^\]]+)\]\]/g;
const MD_LINK = /\]\(([^)\s]+\.md(?:#[^)\s]*)?)\)/g;
/** A path or a symbol. A route (`/vehicles`) or a permission (`vehicle:read`) is neither. */
const CODE_NAME = /^[^\s/:][^\s:]*$/;

function groupOf(row: ListRow | undefined): Group {
  if (row !== undefined && ARCH_TYPES.has(row.type)) return "architecture";
  if (row !== undefined && WORK_TYPES.has(row.type)) return "tasks";
  return "documents";
}

/** `open-items` reads `Open items`. */
function sectionName(anchor: string): string {
  const words = anchor.replace(/-+/g, " ").trim();
  return `${words.slice(0, 1).toUpperCase()}${words.slice(1)}`;
}

function incoming(link: Backlink): string {
  switch (link.kind) {
    case "links":
      return "Lists this document in its links";
    case "diagram":
      return "Shows this document in a diagram";
    case "uses":
      return "Uses this document";
    case "wikilink":
    case "md":
      return link.anchor === null ? "Mentions this document" : `Mentions this document in "${sectionName(link.anchor)}"`;
    default:
      return "Points to this document";
  }
}

function mentioned(title: string | undefined): string {
  return title === undefined ? "Mentioned in the text" : `Mentioned in "${title}"`;
}

/** Two reasons and how many more, so an entry stays one line. */
function relationText(reasons: string[]): string {
  const rest = reasons.length - 2;
  return [...reasons.slice(0, 2), ...(rest > 0 ? [`and ${rest} more`] : [])].join(" · ");
}

/** Everything a doc connects to, out of what it names and what names it. One entry each, the strongest reason first. */
export function connectionsOf({ self, facts, links, body, backlinks, rows, resolver }: ConnectionSource): Connections {
  const byId = new Map(rows.map((row) => [row.id, row]));
  const found = new Map<string, { group: Group; entry: Omit<Connection, "relation">; reasons: string[] }>();

  const add = (group: Group, entry: Omit<Connection, "relation">, reason: string) => {
    const known = found.get(entry.key);
    if (known === undefined) found.set(entry.key, { group, entry, reasons: [reason] });
    else if (!known.reasons.includes(reason)) known.reasons.push(reason);
  };
  const addItem = (id: string, reason: string) => {
    if (id === self) return;
    const row = byId.get(id);
    const group = groupOf(row);
    add(
      group,
      {
        key: id,
        title: row?.title ?? row?.short ?? id,
        href: group === "architecture" ? serviceHref(id) : hrefFor(id),
        ...(group === "tasks" && row?.short ? { short: row.short } : {}),
        ...(group === "tasks" && row?.status ? { status: row.status } : {}),
      },
      reason,
    );
  };
  const addTarget = (target: string, reason: string) => {
    const id = resolver.wiki(target)?.id;
    if (id !== undefined) addItem(id, reason);
  };
  const addCode = (text: string, reason: string) => {
    add("architecture", { key: `code:${text}`, title: shortCode(text), hint: text, href: codeHref(codeTarget(text)) }, reason);
  };

  for (const fact of facts) {
    for (const part of parseFact(fact.value).parts) {
      if (part.kind === "link") addTarget(part.target, FACT_RELATIONS[fact.key] ?? `Named under "${fact.key}"`);
      else if (CODE_NAME.test(part.text)) addCode(part.text, CODE_RELATIONS[fact.key] ?? `Named under "${fact.key}"`);
    }
  }
  for (const link of links) addTarget(link, "This document links to it");
  for (const line of bodyLines(body)) {
    if (line.fenced) continue;
    const text = line.text.replace(/`[^`]*`/g, "");
    const reason = mentioned(line.section?.title);
    for (const match of text.matchAll(WIKILINK)) {
      const target = (match[1] ?? "").trim();
      if (target.startsWith("code:")) addCode(target.slice("code:".length), reason);
      else addTarget(target, reason);
    }
    for (const match of text.matchAll(MD_LINK)) {
      const id = resolver.md(match[1] ?? "", self)?.id;
      if (id !== undefined) addItem(id, reason);
    }
  }
  for (const link of backlinks) addItem(link.id, incoming(link));

  const out: Connections = { documents: [], tasks: [], architecture: [] };
  for (const { group, entry, reasons } of found.values()) out[group].push({ ...entry, relation: relationText(reasons) });
  return out;
}
