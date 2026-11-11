import { cn } from "@/lib/utils";

export type Tone = "todo" | "draft" | "progress" | "review" | "done" | "blocked" | "archived";

const TONES: Record<string, Tone> = {
  draft: "draft",
  "in-progress": "progress",
  active: "progress",
  doing: "progress",
  running: "progress",
  closing: "done",
  paused: "todo",
  planned: "todo",
  review: "review",
  "in-review": "review",
  proposed: "review",
  done: "done",
  accepted: "done",
  stable: "done",
  complete: "done",
  completed: "done",
  shipped: "done",
  blocked: "blocked",
  rejected: "blocked",
  failed: "blocked",
  deprecated: "blocked",
  archived: "archived",
  superseded: "archived",
  cancelled: "archived",
  canceled: "archived",
};

/** The colour family of any status a store can name; an unknown status reads as waiting. */
export function toneOf(status: string | null | undefined): Tone {
  return TONES[(status ?? "").toLowerCase()] ?? "todo";
}

/** Work first: what moves now, then what waits, then what is over. */
const RANK: Record<Tone, number> = { blocked: 0, progress: 1, review: 2, todo: 3, draft: 4, done: 5, archived: 6 };

export function toneRank(status: string | null | undefined): number {
  return RANK[toneOf(status)];
}

const BADGE: Record<Tone, string> = {
  todo: "bg-st-todo-soft text-st-todo",
  draft: "bg-st-draft-soft text-st-draft",
  progress: "bg-st-progress-soft text-st-progress",
  review: "bg-st-review-soft text-st-review",
  done: "bg-st-done-soft text-st-done",
  blocked: "bg-st-blocked-soft text-st-blocked",
  archived: "bg-st-archived-soft text-st-archived",
};

export const DOT: Record<Tone, string> = {
  todo: "bg-st-todo",
  draft: "bg-st-draft",
  progress: "bg-st-progress",
  review: "bg-st-review",
  done: "bg-st-done",
  blocked: "bg-st-blocked",
  archived: "bg-st-archived",
};

export function StatusDot({ status, className }: { status: string | null | undefined; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-2 shrink-0 rounded-full", DOT[toneOf(status)], className)} />;
}

export function StatusBadge({ status }: { status: string | null }) {
  if (status === null) return null;
  return (
    <span className={cn("inline-flex shrink-0 items-center rounded-[5px] px-2 py-0.5 text-[12.5px] font-medium whitespace-nowrap", BADGE[toneOf(status)])}>
      {status}
    </span>
  );
}

const PRIORITY: Record<string, string> = {
  critical: "border-st-blocked/40 bg-st-blocked/10 text-st-blocked",
  urgent: "border-st-blocked/40 bg-st-blocked/10 text-st-blocked",
  high: "border-st-blocked/30 text-st-blocked",
  medium: "border-st-progress/35 text-st-progress",
  low: "border-border text-muted-foreground",
};

export function PriorityBadge({ priority }: { priority: string | null | undefined }) {
  if (!priority) return null;
  return (
    <span className={cn("rounded border px-1.5 py-px font-mono text-[0.6875rem] whitespace-nowrap", PRIORITY[priority.toLowerCase()] ?? PRIORITY.low)}>
      {priority}
    </span>
  );
}

export interface Tally {
  done: number;
  review: number;
  progress: number;
  waiting: number;
  total: number;
}

/** Counts tasks by how far they are; archived tasks count for nothing. */
export function tally(statuses: (string | null)[]): Tally {
  const out: Tally = { done: 0, review: 0, progress: 0, waiting: 0, total: 0 };
  for (const status of statuses) {
    const tone = toneOf(status);
    if (tone === "archived") continue;
    out.total += 1;
    if (tone === "done") out.done += 1;
    else if (tone === "review") out.review += 1;
    else if (tone === "progress") out.progress += 1;
    else out.waiting += 1;
  }
  return out;
}

/** One bar for a group of tasks: done, then in review, then in progress, over a rest that waits. */
export function StackedBar({ tally: t, label, className }: { tally: Tally; label: string; className?: string }) {
  const part = (n: number) => (t.total === 0 ? "0%" : `${(n / t.total) * 100}%`);
  return (
    <span
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={t.total}
      aria-valuenow={t.done}
      className={cn("flex h-1.5 overflow-hidden rounded-full bg-muted", className)}
    >
      <span className="h-full bg-st-done transition-[width] duration-500" style={{ width: part(t.done) }} />
      <span className="h-full bg-st-review transition-[width] duration-500" style={{ width: part(t.review) }} />
      <span className="h-full bg-st-progress transition-[width] duration-500" style={{ width: part(t.progress) }} />
    </span>
  );
}
