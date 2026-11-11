import type { ListRow } from "@/api/types/ListRow";

/** Where the owner's "hide" is remembered, so the panel stays away in this browser. */
export const HIDDEN_KEY = "stone.firstRun.hidden";

/** The doc that most likely holds the product requirements: `docs/prd`, else the first doc titled like one. */
export function prdOf(rows: ListRow[]): ListRow | undefined {
  const docs = rows.filter((row) => row.type === "doc");
  const named = docs.find((row) => row.id === "docs/prd");
  if (named) return named;
  return docs.find((row) => /\bPRD\b|product requirements/i.test(row.title ?? ""));
}

export interface Found {
  label: string;
  count: number;
}

/** The four tiles of what stone already found, in the order the board draws them. */
export function foundCounts(tasks: ListRow[], milestones: ListRow[], decisions: ListRow[], docs: ListRow[]): Found[] {
  return [
    { label: "tasks", count: tasks.length },
    { label: "milestones", count: milestones.length },
    { label: "decisions", count: decisions.length },
    { label: "docs", count: docs.length },
  ];
}

/** True when the owner hid the panel here. A browser that blocks its store reads as not hidden. */
export function hidden(): boolean {
  try {
    return localStorage.getItem(HIDDEN_KEY) === "1";
  } catch {
    return false;
  }
}

/** Remember the hide; a blocked store only means the panel comes back on the next visit. */
export function hide(): void {
  try {
    localStorage.setItem(HIDDEN_KEY, "1");
  } catch {
    // Nothing to do: the panel is hidden for this visit through state.
  }
}
