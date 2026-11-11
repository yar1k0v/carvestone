import type { DocRow } from "@/api/types/DocRow";

const LABELS: Record<string, string> = {
  lld: "Low-level design",
  feature: "Features",
  release: "Releases",
  spec: "Specs",
  runbook: "Runbooks",
  research: "Research",
  conventions: "Conventions",
  template: "Templates",
  other: "Other",
};

/** What a reader calls a family of docs. */
export function groupLabel(group: string): string {
  return LABELS[group] ?? `${group.slice(0, 1).toUpperCase()}${group.slice(1)}`;
}

/** `lld` and `flow` read `Low-level design · flow`; a doc of no family reads as its kind alone. */
export function chipText(row: Pick<DocRow, "group" | "kind">): string {
  const family = row.group === "other" ? null : groupLabel(row.group);
  return [family, row.kind].filter(Boolean).join(" · ") || "Doc";
}

function dayOf(iso: string): Date {
  return new Date(`${iso}T00:00:00`);
}

/** Whole days from `iso` to `now`; a day ahead of the clock counts as today. */
export function daysSince(iso: string, now: Date): number {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.max(0, Math.round((today.getTime() - dayOf(iso).getTime()) / 86_400_000));
}

function count(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/** `today`, `yesterday`, `3 days ago`, `2 weeks ago`, `5 months ago`. */
export function ago(iso: string, now: Date): string {
  const days = daysSince(iso, now);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 14) return count(days, "day");
  if (days < 60) return count(Math.floor(days / 7), "week");
  if (days < 365) return count(Math.floor(days / 30), "month");
  return count(Math.floor(days / 365), "year");
}

export interface DateGroup {
  label: string;
  rows: DocRow[];
}

function bucket(days: number | null): string {
  if (days === null) return "Older";
  if (days === 0) return "Today";
  if (days <= 7) return "This week";
  if (days <= 30) return "This month";
  return "Older";
}

/** Rows under date headings, keeping their order; a heading with no rows is left out. */
export function byDate(rows: DocRow[], now: Date): DateGroup[] {
  const out: DateGroup[] = [];
  for (const row of rows) {
    const label = bucket(row.updated === undefined ? null : daysSince(row.updated, now));
    const last = out.find((group) => group.label === label);
    if (last) last.rows.push(row);
    else out.push({ label, rows: [row] });
  }
  return out;
}

export interface LibraryFilter {
  q?: string;
  group?: string;
  templates?: boolean;
}

/** True when every word of `q` is in the title or the summary. */
export function matchesText(row: { title: string; summary?: string }, q: string | undefined): boolean {
  const words = (q ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const text = `${row.title} ${row.summary ?? ""}`.toLowerCase();
  return words.every((word) => text.includes(word));
}

/** Blanks to copy are not documents to read, so they stay out unless asked for. */
export function readable(rows: DocRow[], templates: boolean | undefined): DocRow[] {
  return templates ? rows : rows.filter((row) => !row.template);
}

/** The docs a filter keeps. `found` adds the docs a search found in their text; `inPart` keeps only the docs of one part. */
export function filterDocs(rows: DocRow[], filter: LibraryFilter, found?: Map<string, unknown>, inPart?: Set<string>): DocRow[] {
  return readable(rows, filter.templates).filter(
    (row) =>
      (filter.group === undefined || row.group === filter.group) &&
      (inPart === undefined || inPart.has(row.id)) &&
      (matchesText(row, filter.q) || found?.has(row.id) === true),
  );
}

/** The families with how many docs each holds, largest first, `Other` last. */
export function groupCounts(rows: DocRow[]): { value: string; label: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const row of rows) counts.set(row.group, (counts.get(row.group) ?? 0) + 1);
  return [...counts]
    .map(([value, n]) => ({ value, label: groupLabel(value), count: n }))
    .sort((a, b) => Number(a.value === "other") - Number(b.value === "other") || b.count - a.count || a.label.localeCompare(b.label));
}

export function byName(rows: DocRow[]): DocRow[] {
  return [...rows].sort((a, b) => a.title.localeCompare(b.title));
}

const RECENT_KEY = "stone.recent-docs";
const RECENT_MAX = 6;

/** The docs this browser opened last, newest first; empty where storage is blocked. */
export function recentDocs(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

export function rememberDoc(id: string): void {
  try {
    const next = [id, ...recentDocs().filter((other) => other !== id)].slice(0, RECENT_MAX);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // A browser that refuses storage only loses the shortcut.
  }
}

let lastLibrary = "/docs";

/** Where "Library" leads back to: the library as the reader left it. */
export function libraryHref(): string {
  return lastLibrary;
}

export function rememberLibrary(href: string): void {
  lastLibrary = href;
}
