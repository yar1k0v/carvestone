import { useMemo } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { useItemRows, useResolver } from "@/components/markdown/useResolver";
import { toneOf } from "@/components/ui/status";
import { isOver } from "@/components/work/taskModel";

/** An open task that cannot move, and the open tasks it waits for. */
export interface Blocker {
  task: ListRow;
  /** Empty when the task's own status says it is blocked. */
  waitingOn: ListRow[];
}

/** Open tasks marked blocked, or whose `depends_on` names a task still open. */
export function findBlockers(tasks: ListRow[], dependencies: (task: ListRow) => ListRow[]): Blocker[] {
  return tasks.flatMap((task) => {
    if (isOver(task.status)) return [];
    const waitingOn = dependencies(task).filter((row) => !isOver(row.status));
    return waitingOn.length > 0 || toneOf(task.status) === "blocked" ? [{ task, waitingOn }] : [];
  });
}

/** The blockers among `tasks`. */
export function useBlockers(tasks: ListRow[]): Blocker[] {
  const rows = useItemRows();
  const resolver = useResolver();
  const open = useMemo(() => tasks.filter((task) => !isOver(task.status)), [tasks]);
  const byId = useMemo(() => new Map((rows.data ?? []).map((row) => [row.id, row])), [rows.data]);
  return findBlockers(open, (task) =>
    (task.depends_on ?? []).flatMap((target) => {
      const id = resolver.wiki(target)?.id;
      const row = id === undefined ? undefined : byId.get(id);
      return row === undefined ? [] : [row];
    }),
  );
}
