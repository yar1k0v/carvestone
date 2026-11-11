import type { ListRow } from "@/api/types/ListRow";
import { toneOf } from "@/components/ui/status";

/** The board filters as the URL keeps them. `show` is the comma list of tucked statuses opened as columns. */
export interface BoardFilters {
  milestone?: string;
  service?: string;
  priority?: string;
  q?: string;
  show?: string;
}

/** The list query the board sends; only the milestone is the server's filter. */
export function boardListParams(filters: BoardFilters) {
  return { type: "task", milestone: filters.milestone };
}

function numberOf(row: ListRow): number {
  const match = /(\d+)$/.exec(row.short ?? row.id);
  return match?.[1] === undefined ? Number.MAX_SAFE_INTEGER : Number(match[1]);
}

/** Text on id and title, one service and one priority; a filter left empty keeps every row. */
export function matchesFilters(row: ListRow, filters: BoardFilters): boolean {
  const needle = (filters.q ?? "").trim().toLowerCase();
  if (needle !== "" && ![row.id, row.short ?? "", row.title ?? ""].some((value) => value.toLowerCase().includes(needle))) return false;
  if (filters.service !== undefined && !(row.services ?? []).includes(filters.service)) return false;
  return filters.priority === undefined || row.priority === filters.priority;
}

/** Distinct services across the rows, sorted. */
export function servicesOf(rows: ListRow[]): string[] {
  const names = new Set<string>();
  for (const row of rows) {
    for (const name of row.services ?? []) names.add(name);
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** Draft, done and archived tasks sit behind a count until asked for. */
export function isTucked(status: string): boolean {
  return ["draft", "done", "archived"].includes(toneOf(status));
}

export function shownStatuses(show: string | undefined): string[] {
  return (show ?? "").split(",").filter((status) => status !== "");
}

/** The `show` value with `status` added or taken out; `undefined` when nothing is left. */
export function toggleShown(show: string | undefined, status: string): string | undefined {
  const shown = shownStatuses(show);
  const next = shown.includes(status) ? shown.filter((name) => name !== status) : [...shown, status];
  return next.length === 0 ? undefined : next.join(",");
}

export function columnLabel(status: string): string {
  if (status === "todo") return "To do";
  if (status === "review") return "In review";
  const words = status.replaceAll("-", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

const DAY = 86_400_000;

function utcDay(year: number, month: number, day: number): number {
  return Date.UTC(year, month - 1, day);
}

/** How long ago a task was created, in whole UTC days. `null` for a missing, invalid or future date. */
export function ageOf(created: string | undefined, today: Date): string | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(created ?? "");
  if (match === null) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const start = utcDay(year, month, day);
  const parsed = new Date(start);
  if (parsed.getUTCFullYear() !== year || parsed.getUTCMonth() + 1 !== month || parsed.getUTCDate() !== day) return null;
  const days = Math.floor((utcDay(today.getUTCFullYear(), today.getUTCMonth() + 1, today.getUTCDate()) - start) / DAY);
  if (days < 0) return null;
  const count = (n: number, unit: string) => `${n} ${unit}${n === 1 ? "" : "s"}`;
  if (days === 0) return "today";
  if (days < 7) return count(days, "day");
  if (days < 60) return count(Math.floor(days / 7), "week");
  return count(Math.floor(days / 30), "month");
}

export interface Column {
  status: string;
  rows: ListRow[];
}

/**
 * One column per configured status (plus any other status a task carries), each ordered by
 * priority as the config lists them, then by task number.
 */
export function groupByStatus(rows: ListRow[], statuses: string[], priorities: string[]): Column[] {
  const rank = (row: ListRow) => {
    const at = row.priority === null ? -1 : priorities.indexOf(row.priority);
    return at < 0 ? priorities.length : at;
  };
  const known = [...statuses];
  for (const row of rows) {
    if (row.status !== null && !known.includes(row.status)) known.push(row.status);
  }
  return known.map((status) => ({
    status,
    rows: rows.filter((row) => row.status === status).sort((a, b) => rank(a) - rank(b) || numberOf(a) - numberOf(b)),
  }));
}

/** `N open tasks across M milestones`, counting the columns that are not tucked. */
export function openSummary(columns: Column[]): string {
  const open = columns.filter((column) => !isTucked(column.status)).flatMap((column) => column.rows);
  const milestones = new Set(open.flatMap((row) => (row.milestone === undefined ? [] : [row.milestone])));
  const tasks = `${open.length} open task${open.length === 1 ? "" : "s"}`;
  if (milestones.size === 0) return tasks;
  return `${tasks} across ${milestones.size} milestone${milestones.size === 1 ? "" : "s"}`;
}
