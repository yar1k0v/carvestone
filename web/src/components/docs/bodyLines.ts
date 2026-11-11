import { slugs } from "@/components/markdown/slug";

export interface BodySection {
  title: string;
  /** The heading's anchor, as the rendered page spells it. */
  anchor: string;
}

export interface BodyLine {
  text: string;
  /** Inside a fenced block, where links are not links. */
  fenced: boolean;
  /** The nearest heading above, or the line's own; `null` before the first one. */
  section: BodySection | null;
}

const FENCE = /^\s{0,3}(`{3,}|~{3,})/;
const HEADING = /^\s{0,3}#{1,6}[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$/;

/** The lines of a doc, each with the section it sits in. */
export function bodyLines(body: string): BodyLine[] {
  const marked: { text: string; fenced: boolean; heading: string | null }[] = [];
  let fence: string | null = null;
  for (const text of body.split(/\r?\n/)) {
    const mark = FENCE.exec(text)?.[1]?.slice(0, 1) ?? null;
    if (mark !== null && (fence === null || fence === mark)) {
      fence = fence === null ? mark : null;
      marked.push({ text, fenced: true, heading: null });
      continue;
    }
    const heading = fence === null ? (HEADING.exec(text)?.[1] ?? null) : null;
    marked.push({ text, fenced: fence !== null, heading });
  }
  const anchors = slugs(marked.flatMap((line) => (line.heading === null ? [] : [line.heading])));
  let found = 0;
  let section: BodySection | null = null;
  return marked.map(({ text, fenced, heading }) => {
    if (heading !== null) {
      section = { title: heading, anchor: anchors[found] ?? "" };
      found += 1;
    }
    return { text, fenced, section };
  });
}
