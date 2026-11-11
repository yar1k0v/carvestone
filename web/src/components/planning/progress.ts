import type { ListRow } from "@/api/types/ListRow";
import { toneOf } from "@/components/ui/status";

export interface Progress {
  done: number;
  total: number;
}

/** Done over all tasks; archived tasks were dropped from the plan and count for neither. */
export function progress(tasks: ListRow[]): Progress {
  const live = tasks.filter((task) => task.status !== "archived");
  return { done: live.filter((task) => task.status === "done").length, total: live.length };
}

export type MilestoneState = "Planned" | "Running" | "Closing" | "Paused" | "Done";

/**
 * The state a milestone's tasks add up to; first match wins. Nothing open is Done, and open work
 * that only waits for review or a start decides between Closing, Running, Paused and Planned.
 */
export function milestoneState(tasks: ListRow[]): MilestoneState {
  if (tasks.length === 0) return "Planned";
  const tones = tasks.map((task) => toneOf(task.status));
  const finished = tones.filter((tone) => tone === "done" || tone === "archived").length;
  const moving = tones.filter((tone) => tone === "progress" || tone === "review").length;
  const waiting = tones.length - finished - moving;
  if (finished === tones.length) return "Done";
  if (waiting === 0) return "Closing";
  if (moving > 0) return "Running";
  return finished > 0 ? "Paused" : "Planned";
}

/** The id a milestone route uses: `M-2` for `milestones/M-2`. */
export function milestoneRef(row: ListRow): string {
  return row.short ?? row.id.replace(/^milestones\//, "");
}

/** A field value that is plain text, else `null`. */
export function textField(fields: Record<string, unknown>, key: string): string | null {
  const value = fields[key];
  return typeof value === "string" && value !== "" ? value : null;
}
