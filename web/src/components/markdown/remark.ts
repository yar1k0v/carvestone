import type { Heading, Parent, Root, RootContent } from "mdast";
import { slugs } from "./slug";

/** Marks the links the renderer turns into router links and code chips. */
export const WIKI_SCHEME = "wikilink:";
export const CODE_SCHEME = "codelink:";

function collectHeadings(node: Root | Parent, out: Heading[]): void {
  for (const child of node.children) {
    if (child.type === "heading") out.push(child);
    if ("children" in child) collectHeadings(child as Parent, out);
  }
}

/** The heading as written in the source, without its `#` marks: what the engine slugs. */
function rawTitle(source: string, heading: Heading): string {
  const start = heading.position?.start.offset;
  const end = heading.position?.end.offset;
  if (start === undefined || end === undefined) return "";
  const firstLine = source.slice(start, end).split("\n")[0] ?? "";
  return firstLine
    .replace(/^\s{0,3}#{1,6}[ \t]*/, "")
    .replace(/[ \t]+#+[ \t]*$/, "")
    .trim();
}

/** A remark plugin that gives every heading the engine's anchor as its `id`. */
export function headingIds(source: string) {
  return () => (tree: Root) => {
    const found: Heading[] = [];
    collectHeadings(tree, found);
    const anchors = slugs(found.map((heading) => rawTitle(source, heading)));
    found.forEach((heading, i) => {
      heading.data = { ...heading.data, hProperties: { id: anchors[i] } };
    });
  };
}

const WIKILINK = /\[\[([^\]]+)\]\]/g;

function splitText(value: string): RootContent[] {
  const out: RootContent[] = [];
  let last = 0;
  for (const match of value.matchAll(WIKILINK)) {
    const target = (match[1] ?? "").trim();
    if (target === "") continue;
    if (match.index > last) out.push({ type: "text", value: value.slice(last, match.index) });
    const isCode = target.startsWith("code:");
    out.push({
      type: "link",
      url: `${isCode ? CODE_SCHEME : WIKI_SCHEME}${encodeURIComponent(isCode ? target.slice("code:".length) : target)}`,
      children: [{ type: "text", value: isCode ? target.slice("code:".length) : target }],
    });
    last = match.index + match[0].length;
  }
  if (last === 0) return [{ type: "text", value }];
  if (last < value.length) out.push({ type: "text", value: value.slice(last) });
  return out;
}

function rewrite(parent: Parent): void {
  const next: RootContent[] = [];
  for (const child of parent.children) {
    if (child.type === "text") {
      next.push(...splitText(child.value));
      continue;
    }
    // Inline code and fences are not text nodes, so `[[x]]` inside them is left alone, as the engine does.
    if (child.type !== "link" && "children" in child) rewrite(child as Parent);
    next.push(child);
  }
  parent.children = next;
}

/** A remark plugin that turns `[[target]]` into links and `[[code:name]]` into code links. */
export function wikilinks() {
  return (tree: Root) => rewrite(tree);
}
