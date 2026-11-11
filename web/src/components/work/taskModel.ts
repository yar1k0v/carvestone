import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ItemView } from "@/api/types/ItemView";
import type { TaskCommit } from "@/api/types/TaskCommit";
import { bodyLines } from "@/components/docs/bodyLines";
import { toneOf } from "@/components/ui/status";

const GOAL_SECTIONS = ["goal", "description", "summary", "why", "problem"];
const CHECKBOX = /^\s*[-*+]\s+\[([ xX])\]\s+(.*)$/;

export interface TaskCheck {
  text: string;
  checked: boolean;
}

export interface TaskText {
  /** The first paragraph of the section that states the goal, else of the task. */
  goal: string | null;
  checks: TaskCheck[];
}

function firstParagraph(lines: string[]): string | null {
  const start = lines.findIndex((line) => line.trim() !== "");
  if (start < 0) return null;
  const rest = lines.slice(start);
  const end = rest.findIndex((line) => line.trim() === "");
  return rest.slice(0, end < 0 ? undefined : end).join("\n");
}

/** What the top of the task page reads out of the task's text. */
export function readTask(body: string): TaskText {
  const lines = bodyLines(body);
  const checks: TaskCheck[] = [];
  for (const line of lines) {
    const box = line.fenced ? null : CHECKBOX.exec(line.text);
    if (box?.[1] !== undefined && box[2] !== undefined) checks.push({ text: box[2].trim(), checked: box[1] !== " " });
  }
  const plain = lines.filter((line) => !line.fenced);
  for (const name of GOAL_SECTIONS) {
    const inside = plain.filter((line) => line.section?.title.trim().toLowerCase() === name);
    const goal = firstParagraph(inside.slice(1).map((line) => line.text));
    if (goal !== null) return { goal, checks };
  }
  return { goal: firstParagraph(plain.filter((line) => line.section === null).map((line) => line.text)), checks };
}

/** Done or archived: nothing more will happen on it. */
export function isOver(status: string | null | undefined): boolean {
  const tone = toneOf(status);
  return tone === "done" || tone === "archived";
}

/** A list field, as text. */
export function textList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string") : [];
}

/** The ids of the tasks waiting for this one: those whose `depends_on` names it. */
export function blockedIds(item: ItemView): string[] {
  return [...new Set(item.backlinks.filter((link) => link.kind === "depends_on").map((link) => link.id))];
}

/** The commits tied to a task, by path id or short id. */
export function useTaskCommits(id: string) {
  const short = id.replace(/^tasks\//, "");
  return useQuery({
    queryKey: keys.taskCommits(short),
    queryFn: () => api.get<TaskCommit[]>(`/api/tasks/${encodeURIComponent(short)}/commits`),
  });
}

/** `[[plans/x.md]]` as a resolver takes it. */
export function refTarget(value: string): string {
  return value.replace(/^\[\[|\]\]$/g, "");
}
