import type { Hit } from "@/api/types/Hit";

/** A source position as text: `src/a.rs:10-24`. No editor opens; the explorer only names the place. */
export function sourceOf(hit: Pick<Hit, "path" | "lines">): string {
  const [start, end] = hit.lines;
  return start === end ? `${hit.path}:${start}` : `${hit.path}:${start}-${end}`;
}
