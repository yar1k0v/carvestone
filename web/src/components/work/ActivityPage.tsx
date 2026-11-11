import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ActivityGroup } from "@/api/types/ActivityGroup";
import type { ChangedItem } from "@/api/types/ChangedItem";
import type { DoneTask } from "@/api/types/DoneTask";
import type { ListRow } from "@/api/types/ListRow";
import { Empty } from "@/components/architecture/service/parts";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor, taskHref } from "@/components/markdown/links";
import { TaskLink } from "@/components/peek/TaskLink";
import { type Work, useWork } from "@/components/planning/useWork";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { useOutline } from "@/shell/pageFrame";
import { type ActivitySearch, PERIODS, type Period, type Topic, earlier, plural, topicsOf } from "./activityModel";

function useActivity(period: Period, from: string | undefined) {
  const params = { period, from };
  return useQuery({
    queryKey: keys.activity(params),
    queryFn: () => api.get<ActivityGroup[]>("/api/activity", params),
    // Reading further back keeps what is on screen; another tab starts clean.
    placeholderData: (previous, query) => (query?.queryKey[2].period === period ? previous : undefined),
  });
}

function counts(group: ActivityGroup): string {
  return [
    plural(group.tasks_done.length, "task finished", "tasks finished"),
    plural(group.docs_changed.length, "doc changed", "docs changed"),
    plural(group.decisions_changed.length, "decision changed", "decisions changed"),
    plural(group.commit_count, "commit"),
  ].join(" · ");
}

function Changes({ title, rows }: { title: string; rows: ChangedItem[] }) {
  if (rows.length === 0) return null;
  return (
    <div>
      <h3 className="mb-1 text-sm font-medium">{title}</h3>
      <ul className="text-sm">
        {rows.map((row) => (
          <li key={row.id} className="flex items-baseline gap-3 py-0.5">
            <AppLink href={hrefFor(row.id)} className="min-w-0 flex-1 hover:text-primary">
              {row.title}
            </AppLink>
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{row.day}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function TaskTitle({ task, row }: { task: DoneTask; row: ListRow | undefined }) {
  const className = "min-w-0 flex-1 hover:underline";
  return row ? (
    <TaskLink row={row} className={className}>
      {task.title}
    </TaskLink>
  ) : (
    <AppLink href={taskHref(task.id)} className={className}>
      {task.title}
    </AppLink>
  );
}

function TopicBlock({ topic }: { topic: Topic }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h3 className="text-[15px] font-semibold">
        {topic.name} <span className="font-mono text-xs font-medium text-muted-foreground">{topic.rows.length}</span>
      </h3>
      <ul className="slab divide-y overflow-hidden">
        {topic.rows.map(({ task, row }) => (
          <li key={task.id} className="flex items-baseline gap-3 px-3.5 py-[9px]">
            <span className="w-[54px] shrink-0 font-mono text-xs text-muted-foreground">{task.id.replace(/^tasks\//, "")}</span>
            <TaskTitle task={task} row={row} />
            <span className="shrink-0 font-mono text-xs text-muted-foreground tabular-nums">{task.closed}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Group({ group, work, loading }: { group: ActivityGroup; work: Pick<Work, "tasks" | "milestones">; loading: boolean }) {
  const anchor = `period-${group.start}`;
  const topics = useMemo(() => topicsOf(group.tasks_done, work.tasks, work.milestones), [group.tasks_done, work.tasks, work.milestones]);
  return (
    <section aria-labelledby={anchor} className="border-t py-6">
      <h2 id={anchor} className="scroll-mt-6 text-lg font-semibold">
        {group.label}
      </h2>
      <p className="mt-0.5 mb-3 text-muted-foreground tabular-nums">{counts(group)}</p>
      <div className="space-y-4">
        {/* Until the item list arrives, no task has a milestone to show, and none may read as lacking one. */}
        {loading ? (
          <Loading lines={3} label="Reading the milestones..." className="max-w-xl" />
        ) : (
          topics.map((topic) => <TopicBlock key={topic.name} topic={topic} />)
        )}
        <Changes title="Docs changed" rows={group.docs_changed} />
        <Changes title="Decisions changed" rows={group.decisions_changed} />
      </div>
    </section>
  );
}

const TAB = "rounded-[7px] px-[11px] py-[5px] text-[13px]";

function Tabs({ active, onSelect }: { active: Period; onSelect: (period: Period) => void }) {
  return (
    <div role="tablist" aria-label="Period" className="flex flex-wrap gap-2">
      {PERIODS.map((period) => (
        <button
          key={period.value}
          role="tab"
          type="button"
          aria-selected={period.value === active}
          onClick={() => onSelect(period.value)}
          className={cn(TAB, period.value === active ? "bg-primary/10 text-primary" : "border bg-card hover:text-primary")}
        >
          {period.label}
        </button>
      ))}
    </div>
  );
}

/** Work done by day, week or month, newest first. */
export function ActivityPage({ search, onSearch }: { search: ActivitySearch; onSearch: (next: ActivitySearch) => void }) {
  const period = search.period ?? "week";
  const query = useActivity(period, search.from);
  const work = useWork();
  const groups = query.data ?? [];
  const shownFrom = groups[groups.length - 1]?.start ?? new Date().toISOString().slice(0, 10);
  // From what was asked for, or a quiet stretch longer than one step would ask the same thing again.
  const oldest = search.from !== undefined && search.from < shownFrom ? search.from : shownFrom;
  useOutline(groups.map((group) => ({ id: `period-${group.start}`, title: group.label })));

  return (
    <div className="pb-8 text-sm leading-[1.4]">
      <PageHeader title="Shipped" actions={<Tabs active={period} onSelect={(next) => onSearch({ period: next })} />} />
      <div role="tabpanel">
        {query.isPending ? <Loading lines={6} label="Reading the work done..." className="max-w-xl" /> : null}
        {query.error ? <ErrorNotice error={query.error} /> : null}
        {work.error ? <ErrorNotice error={work.error} /> : null}
        {query.data && groups.length === 0 ? <Empty>Nothing was finished or changed in this stretch.</Empty> : null}
        {groups.map((group) => (
          <Group key={group.start} group={group} work={work} loading={work.isPending} />
        ))}
        {query.data ? (
          <Button variant="outline" size="sm" className="mt-2" onClick={() => onSearch({ period, from: earlier(period, oldest) })}>
            Show earlier
          </Button>
        ) : null}
      </div>
    </div>
  );
}
