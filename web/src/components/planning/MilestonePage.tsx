import { useConfig } from "@/api/useConfig";
import type { ListRow } from "@/api/types/ListRow";
import { Block, Empty, Flag } from "@/components/architecture/service/parts";
import { dropTitle } from "@/components/docs/lead";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { isItemView, useCachedRow, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { useGo, useResolver } from "@/components/markdown/useResolver";
import { TaskLink } from "@/components/peek/TaskLink";
import { Loading, LoadingRows } from "@/components/ui/skeleton";
import { StackedBar, tally } from "@/components/ui/status";
import { useOutline } from "@/shell/pageFrame";
import { StatusBadge, TaskGroups } from "./TaskRows";
import { type Blocker, useBlockers } from "./blockers";
import { milestoneState, textField } from "./progress";
import { useMilestoneTasks } from "./useRows";
import { refKey, useWork } from "./useWork";

const NO_TASKS: ListRow[] = [];

const OUTLINE = [
  { id: "milestone-plan", title: "Plan" },
  { id: "milestone-blockers", title: "Blockers" },
  { id: "milestone-tasks", title: "Tasks" },
];

function Plans({ id }: { id: string }) {
  const work = useWork();
  const plans = work.plans.filter((row) => row.milestone !== undefined && refKey(row.milestone) === refKey(id));
  if (work.isPending) return <LoadingRows rows={1} />;
  if (plans.length === 0) return <Empty>No plan names this milestone yet.</Empty>;
  return (
    <ul aria-label="Plans" className="slab divide-y overflow-hidden">
      {plans.map((row) => {
        const t = tally((work.byPlan.get(refKey(row.id)) ?? []).map((task) => task.status));
        return (
          <li key={row.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3.5 py-2 text-sm">
            <AppLink href={`/plans/${row.id.replace(/^plans\//, "")}`} className="min-w-0 flex-1 basis-48 font-medium hover:text-primary">
              {row.title ?? row.id}
            </AppLink>
            <StatusBadge status={row.status} />
            <span className="text-xs text-muted-foreground tabular-nums">
              {t.done}/{t.total}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

function Blockers({ blockers }: { blockers: Blocker[] }) {
  if (blockers.length === 0) return <Empty>No open task here is blocked.</Empty>;
  return (
    <ul aria-label="Blockers" className="slab divide-y overflow-hidden">
      {blockers.map(({ task, waitingOn }) => (
        <li key={task.id} className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 px-3.5 py-2 text-sm">
          <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground">{task.short ?? task.id}</span>
          <TaskLink row={task} className="min-w-0 flex-1 basis-48 font-medium hover:underline">
            {task.title ?? task.id}
          </TaskLink>
          <Flag>{waitingOn.length === 0 ? "blocked" : `waits for ${waitingOn.map((row) => row.short ?? row.id).join(", ")}`}</Flag>
        </li>
      ))}
    </ul>
  );
}

export function MilestonePage({ id }: { id: string }) {
  const item = useItemQuery(`milestones/${id}`);
  const cached = useCachedRow(`milestones/${id}`);
  const tasks = useMilestoneTasks(id);
  const config = useConfig();
  const resolver = useResolver();
  const go = useGo();
  const rows = tasks.data ?? NO_TASKS;
  const blockers = useBlockers(rows);
  // Until the tasks arrive the state would read Planned, which is not true yet.
  const state = tasks.data ? milestoneState(rows) : null;
  useOutline(OUTLINE);

  if (item.isPending) {
    return (
      <article className="space-y-6">
        <header>
          <p className="eyebrow">{id}</p>
          <h1 className="text-2xl font-semibold">{cached?.title ?? id}</h1>
          <div className="mt-2">
            <StatusBadge status={state} />
          </div>
        </header>
        <Loading lines={5} />
      </article>
    );
  }
  if (item.isError) return <ErrorNotice error={item.error} />;
  if (!isItemView(item.data)) return <ErrorNotice error={new Error(`${id} is not a milestone`)} />;

  const view = item.data;
  const t = tally(rows.map((row) => row.status));
  const target = textField(view.fields, "target");
  const title = textField(view.fields, "title") ?? view.id;

  return (
    <article className="pb-8">
      <header className="mb-6">
        <p className="eyebrow">{view.short ?? view.id}</p>
        <h1 className="text-2xl font-semibold">{title}</h1>
        <div className="mt-2 flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
          <StatusBadge status={state} />
          <span>{target ? `target ${target}` : "no target"}</span>
          <span className="tabular-nums">
            {t.done}/{t.total} tasks done
          </span>
          <AppLink href={`/items/${view.id}`} className="underline underline-offset-2">
            Open as item
          </AppLink>
        </div>
        <StackedBar tally={t} label="Milestone progress" className="mt-4 h-2 max-w-xl" />
      </header>
      <Markdown source={dropTitle(view.body, textField(view.fields, "title") ?? "")} resolver={resolver} fromId={view.id} onNavigate={go} />
      <div className="mt-6">
        <Block id="milestone-plan" title="Plan">
          <Plans id={id} />
        </Block>
        <Block id="milestone-blockers" title="Blockers">
          <Blockers blockers={blockers} />
        </Block>
        <Block id="milestone-tasks" title="Tasks">
          {tasks.isPending ? <LoadingRows rows={4} /> : null}
          {tasks.error ? <ErrorNotice error={tasks.error} /> : null}
          {tasks.data && rows.length === 0 ? <p className="text-sm text-muted-foreground">No tasks in this milestone.</p> : null}
          <TaskGroups tasks={rows} statuses={config.data?.statuses.task} />
        </Block>
      </div>
    </article>
  );
}
