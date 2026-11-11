import { useMemo } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { Flag } from "@/components/architecture/service/parts";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { PageHeader } from "@/components/ui/page";
import { LoadingRows } from "@/components/ui/skeleton";
import { StackedBar, StatusBadge, tally, toneOf } from "@/components/ui/status";
import { isOver } from "@/components/work/taskModel";
import { type Blocker, useBlockers } from "./blockers";
import { type MilestoneState, milestoneRef, milestoneState } from "./progress";
import { refKey, useWork } from "./useWork";

/** What moves first, what is finished last. */
const STATE_ORDER: MilestoneState[] = ["Running", "Closing", "Paused", "Planned", "Done"];

interface Entry {
  row: ListRow;
  tasks: ListRow[];
  state: MilestoneState;
}

function blockerHint(blockers: Blocker[]): string {
  return blockers
    .map(({ task, waitingOn }) => {
      const name = task.short ?? task.id;
      return waitingOn.length === 0 ? `${name} is blocked` : `${name} waits for ${waitingOn.map((row) => row.short ?? row.id).join(", ")}`;
    })
    .join("\n");
}

const NUMBER = "px-3 py-3.5 text-right font-mono tabular-nums";

function MilestoneRow({ entry, blockers }: { entry: Entry; blockers: Blocker[] }) {
  const { row, tasks, state } = entry;
  const ref = milestoneRef(row);
  const t = tally(tasks.map((task) => task.status));
  const percent = t.total === 0 ? 0 : Math.round((t.done / t.total) * 100);
  return (
    <tr className="border-t transition-colors hover:bg-accent/50">
      <th scope="row" className="px-4 py-3.5 text-left font-medium">
        <span className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="w-14 shrink-0 font-mono text-xs font-normal text-muted-foreground">{ref}</span>
          <AppLink href={`/milestones/${ref}`} className="min-w-0 hover:underline">
            {row.title ?? row.id}
          </AppLink>
          {blockers.length === 0 ? null : (
            <span title={blockerHint(blockers)}>
              <Flag>{blockers.length} blocked</Flag>
            </span>
          )}
        </span>
      </th>
      <td className="px-3 py-3.5">
        <StatusBadge status={state} />
      </td>
      <td className={NUMBER}>{t.waiting}</td>
      <td className={NUMBER}>{t.progress}</td>
      <td className={NUMBER}>{t.review}</td>
      <td className={NUMBER}>{t.done}</td>
      <td className="px-4 py-3.5">
        <span className="flex items-center gap-2.5 text-xs">
          <StackedBar tally={t} label={`${ref} progress`} className="flex-1" />
          <span className="w-9 text-right font-mono tabular-nums">{percent}%</span>
        </span>
      </td>
    </tr>
  );
}

const HEAD = "px-3 py-2.5 font-medium";

/** Open work that no milestone claims, which no row above can show. */
function Loose({ tasks }: { tasks: ListRow[] }) {
  if (tasks.length === 0) return null;
  const waiting = tasks.filter((task) => toneOf(task.status) === "review").length;
  return (
    <p className="mt-4 rounded-lg bg-amber-100 px-3.5 py-[11px] text-amber-900 dark:bg-amber-950 dark:text-amber-200">
      <strong className="font-semibold">{tasks.length === 1 ? "1 open task has no milestone." : `${tasks.length} open tasks have no milestone.`}</strong>
      {waiting === 0 ? null : <span> {waiting} of them wait for review.</span>}{" "}
      <AppLink href="/board" className="underline">
        See them on the board
      </AppLink>
    </p>
  );
}

export function MilestoneList() {
  const work = useWork();
  const entries = useMemo(
    () =>
      work.milestones
        .map((row): Entry => {
          const tasks = work.byMilestone.get(refKey(milestoneRef(row))) ?? [];
          return { row, tasks, state: milestoneState(tasks) };
        })
        .sort((a, b) => STATE_ORDER.indexOf(a.state) - STATE_ORDER.indexOf(b.state)),
    [work.milestones, work.byMilestone],
  );
  const running = useMemo(() => entries.filter((entry) => entry.state !== "Done").flatMap((entry) => entry.tasks), [entries]);
  const blockers = useBlockers(running);
  const loose = useMemo(() => work.tasks.filter((task) => !task.milestone && !isOver(task.status)), [work.tasks]);
  return (
    <div className="text-sm leading-[1.4]">
      <PageHeader title="Milestones" description="Where the work stands. State comes from the tasks, so it cannot go stale." />
      {work.isPending ? <LoadingRows /> : null}
      {work.error ? <ErrorNotice error={work.error} /> : null}
      {!work.isPending && !work.error && entries.length === 0 ? <p className="text-sm text-muted-foreground">No milestones yet.</p> : null}
      {entries.length > 0 ? (
        <div className="slab overflow-x-auto">
          <table className="w-full min-w-[820px] border-collapse text-sm">
            <thead>
              <tr className="text-left text-xs text-muted-foreground">
                <th scope="col" className="px-4 py-2.5 font-medium">
                  Milestone
                </th>
                <th scope="col" className={HEAD}>
                  State
                </th>
                <th scope="col" className={`${HEAD} text-right`}>
                  Ready
                </th>
                <th scope="col" className={`${HEAD} text-right`}>
                  Running
                </th>
                <th scope="col" className={`${HEAD} text-right`}>
                  Review
                </th>
                <th scope="col" className={`${HEAD} text-right`}>
                  Done
                </th>
                <th scope="col" className="w-[190px] px-4 py-2.5 font-medium">
                  Progress
                </th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <MilestoneRow key={entry.row.id} entry={entry} blockers={blockers.filter((blocker) => entry.tasks.includes(blocker.task))} />
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
      {work.isPending || work.error ? null : <Loose tasks={loose} />}
    </div>
  );
}
