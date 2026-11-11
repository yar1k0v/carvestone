import { useMemo } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { useItemRows } from "@/components/markdown/useResolver";

/** `[[plans/x.md]]`, `plans/x` and `X` are one plan; `milestones/M-02` and `m-2` are one milestone. */
export function refKey(value: string): string {
  const clean = value.replace(/^\[\[|\]\]$/g, "").replace(/\.md$/, "").split("#")[0] ?? "";
  const last = (clean.slice(clean.lastIndexOf("/") + 1) || clean).toLowerCase();
  const numbered = /^([a-z]+)-0*(\d+)$/.exec(last);
  return numbered ? `${numbered[1]}-${numbered[2]}` : last;
}

function groupBy(tasks: ListRow[], field: (row: ListRow) => string | null | undefined): Map<string, ListRow[]> {
  const out = new Map<string, ListRow[]>();
  for (const task of tasks) {
    const value = field(task);
    if (!value) continue;
    const key = refKey(value);
    const list = out.get(key);
    if (list) list.push(task);
    else out.set(key, [task]);
  }
  return out;
}

export interface Work {
  tasks: ListRow[];
  plans: ListRow[];
  milestones: ListRow[];
  /** Tasks by `refKey` of their milestone. */
  byMilestone: Map<string, ListRow[]>;
  /** Tasks by `refKey` of their plan. */
  byPlan: Map<string, ListRow[]>;
}

/** Plans, milestones and tasks out of the one cached item list: no page needs a call per row. */
export function useWork() {
  const query = useItemRows();
  const work = useMemo<Work>(() => {
    const rows = query.data ?? [];
    const tasks = rows.filter((row) => row.type === "task");
    return {
      tasks,
      plans: rows.filter((row) => row.type === "plan"),
      milestones: rows.filter((row) => row.type === "milestone"),
      byMilestone: groupBy(tasks, (row) => row.milestone),
      byPlan: groupBy(tasks, (row) => row.plan),
    };
  }, [query.data]);
  return { ...work, isPending: query.isPending, error: query.error };
}
