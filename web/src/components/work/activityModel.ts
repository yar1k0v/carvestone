import type { DoneTask } from "@/api/types/DoneTask";
import type { ListRow } from "@/api/types/ListRow";
import { refKey } from "@/components/planning/useWork";

export type Period = "day" | "week" | "month";

export interface ActivitySearch {
  period?: Period;
  /** `YYYY-MM-DD`: read back to this day instead of the recent stretch. */
  from?: string;
}

export const PERIODS: { value: Period; label: string }[] = [
  { value: "day", label: "Day" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
];

export function isPeriod(value: unknown): value is Period {
  return value === "day" || value === "week" || value === "month";
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

export interface Topic {
  name: string;
  rows: { task: DoneTask; row: ListRow | undefined }[];
}

const NO_MILESTONE = "No milestone";

/** Finished tasks grouped by the title of their milestone: the biggest group first, tasks with none last. */
export function topicsOf(done: DoneTask[], tasks: ListRow[], milestones: ListRow[]): Topic[] {
  const titles = new Map(milestones.map((row) => [refKey(row.short ?? row.id), row.title ?? row.id]));
  const groups = new Map<string, Topic>();
  for (const task of done) {
    const row = tasks.find((candidate) => candidate.id === task.id || candidate.short === task.id);
    const name = (row?.milestone ? titles.get(refKey(row.milestone)) : undefined) ?? NO_MILESTONE;
    const topic = groups.get(name);
    if (topic) topic.rows.push({ task, row });
    else groups.set(name, { name, rows: [{ task, row }] });
  }
  const rank = (topic: Topic) => Number(topic.name === NO_MILESTONE);
  return [...groups.values()].sort((a, b) => rank(a) - rank(b) || b.rows.length - a.rows.length || a.name.localeCompare(b.name));
}

/** How far "Show earlier" reads back from `start`: as much again as the recent stretch shows. */
export function earlier(period: Period, start: string): string {
  const day = new Date(`${start}T00:00:00Z`);
  if (period === "day") day.setUTCDate(day.getUTCDate() - 14);
  else if (period === "week") day.setUTCDate(day.getUTCDate() - 56);
  else day.setUTCMonth(day.getUTCMonth() - 6);
  return day.toISOString().slice(0, 10);
}
