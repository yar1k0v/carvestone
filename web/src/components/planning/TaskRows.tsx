import type { ListRow } from "@/api/types/ListRow";
import { TaskLink } from "@/components/peek/TaskLink";
import { PriorityBadge, StatusBadge, toneRank } from "@/components/ui/status";

export { StatusBadge };

/** Task rows that open the task view over the page, each with its priority and status. */
export function TaskRows({ tasks }: { tasks: ListRow[] }) {
  if (tasks.length === 0) return <p className="text-sm text-muted-foreground">No tasks.</p>;
  return (
    <ul className="slab divide-y overflow-hidden">
      {tasks.map((task) => (
        <li key={task.id} className="flex items-center gap-3 px-3 py-2 text-sm transition-colors hover:bg-accent/50">
          <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground">{task.short ?? task.id}</span>
          <TaskLink row={task} className="min-w-0 flex-1 truncate hover:underline">
            {task.title ?? task.id}
          </TaskLink>
          <PriorityBadge priority={task.priority} />
          <StatusBadge status={task.status} />
        </li>
      ))}
    </ul>
  );
}

/** Tasks under one heading per status: what moves first, what is over last. */
export function TaskGroups({ tasks, statuses = [] }: { tasks: ListRow[]; statuses?: string[] }) {
  const seen = [...statuses, ...tasks.map((task) => task.status ?? "")].filter((status) => status !== "");
  const order = [...new Set(seen)].sort((a, b) => toneRank(a) - toneRank(b));
  return (
    <div className="space-y-4">
      {order.map((status) => {
        const group = tasks.filter((task) => task.status === status);
        if (group.length === 0) return null;
        return (
          <div key={status}>
            <h3 className="mb-1.5 text-sm font-medium">
              {status} <span className="text-muted-foreground">{group.length}</span>
            </h3>
            <TaskRows tasks={group} />
          </div>
        );
      })}
    </div>
  );
}
