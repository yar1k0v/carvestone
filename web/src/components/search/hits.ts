import type { DocRow } from "@/api/types/DocRow";
import type { Hit } from "@/api/types/Hit";
import type { ListRow } from "@/api/types/ListRow";
import { chipText } from "@/components/docs/library";
import { codeHref, hrefFor } from "@/components/markdown/links";

/** A hit with the words a person reads for it. */
export interface ShownHit {
  hit: Hit;
  title: string;
  sub: string;
}

export interface HitGroup {
  name: string;
  hits: ShownHit[];
}

/** Product first, code last. */
const GROUP_ORDER = ["Feature", "Tasks", "Docs", "System", "Code"];

const DOC_KINDS = new Set(["doc", "decision", "plan", "milestone", "reference"]);
const SYSTEM_KINDS = new Set(["system", "container", "component"]);

function groupOf(hit: Hit, doc: DocRow | undefined): string {
  if (hit.kind === "doc" && doc?.group === "feature") return "Feature";
  if (hit.kind === "task") return "Tasks";
  if (DOC_KINDS.has(hit.kind)) return "Docs";
  if (SYSTEM_KINDS.has(hit.kind)) return "System";
  return "Code";
}

/** The last part of a symbol id: `orders::Cart.total` reads `total`. */
function symbolName(id: string): string {
  return id.split(/::|\.|\//).filter((part) => part !== "").pop() ?? id;
}

function subOf(hit: Hit, group: string, row: ListRow | undefined, doc: DocRow | undefined): string {
  if (group === "Code") return hit.path;
  if (group === "System") return hit.kind;
  if (group === "Feature" || hit.kind === "doc") return doc ? chipText(doc) : "Doc";
  if (hit.kind === "reference") return "Reference";
  const stated = [row?.short, row?.status].filter((part): part is string => !!part);
  return stated.length > 0 ? stated.join(" · ") : hit.kind;
}

/**
 * The hits under Feature, Tasks, Docs, System and Code, each with its title and a short line.
 * Rows that have not loaded leave the id as the title.
 */
export function groupHits(hits: Hit[], rows: ListRow[], docs: DocRow[]): HitGroup[] {
  const rowById = new Map(rows.map((row) => [row.id, row]));
  const docById = new Map(docs.map((doc) => [doc.id, doc]));
  const byName = new Map<string, ShownHit[]>();
  for (const hit of hits) {
    const row = rowById.get(hit.id);
    const doc = docById.get(hit.id);
    const group = groupOf(hit, doc);
    const title = group === "Code" ? symbolName(hit.id) : (row?.title ?? doc?.title ?? hit.id);
    const shown = byName.get(group) ?? [];
    shown.push({ hit, title, sub: subOf(hit, group, row, doc) });
    byName.set(group, shown);
  }
  return GROUP_ORDER.flatMap((name) => {
    const shown = byName.get(name);
    return shown ? [{ name, hits: shown }] : [];
  });
}

/** Where a hit opens: a symbol in the code explorer, a section on its item or doc page. */
export function hitHref(hit: Hit): string {
  return hit.kind === "symbol" ? codeHref(hit.id) : hrefFor(hit.id, hit.anchor);
}

export interface Piece {
  text: string;
  match: boolean;
}

/** The snippet cut into plain and matching pieces; a match is a word of the query. */
export function highlight(snippet: string, query: string): Piece[] {
  const words = [...new Set(query.toLowerCase().split(/[^\p{L}\p{N}_-]+/u).filter((w) => w !== ""))];
  if (words.length === 0) return [{ text: snippet, match: false }];
  const escaped = words.sort((a, b) => b.length - a.length).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`(${escaped.join("|")})`, "giu");
  return snippet
    .split(pattern)
    .filter((text) => text !== "")
    .map((text) => ({ text, match: words.includes(text.toLowerCase()) }));
}
