import type { ListRow } from "@/api/types/ListRow";
import { useArchGraph } from "@/components/architecture/service/useService";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { TaskLink } from "@/components/peek/TaskLink";
import { milestoneRef, progress } from "@/components/planning/progress";
import { refKey, useWork } from "@/components/planning/useWork";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { Decision } from "@/components/work/OwnerActions";
import { isOver } from "@/components/work/taskModel";
import { FirstRun } from "./FirstRun";

/** Findings shown before the rest are left to the System space. */
const FINDINGS = 5;

const HEADING = "mb-2 text-[15px] font-semibold";
const ROW = "border-t py-2";

function Tile({ href, count, label, alarm }: { href: string; count: number; label: string; alarm?: boolean }) {
  return (
    <AppLink href={href} className="slab flex flex-col px-3.5 py-3">
      <span className={`text-[26px] font-[650] tabular-nums ${alarm && count > 0 ? "text-st-blocked" : ""}`}>{count}</span>
      <span className="text-muted-foreground">{label}</span>
    </AppLink>
  );
}

function Decisions({ review }: { review: ListRow[] }) {
  return (
    <section className="flex min-w-0 flex-col">
      <h2 className={HEADING}>Needs your decision</h2>
      {review.length === 0 ? <p className="text-sm text-muted-foreground">Nothing is waiting for you.</p> : null}
      {review.map((task) => (
        <div key={task.id} className="flex flex-wrap items-center gap-x-1.5 gap-y-1.5 border-t py-[9px]">
          <span className="mr-1.5 w-14 shrink-0 font-mono text-xs text-muted-foreground">{task.short ?? task.id}</span>
          <TaskLink row={task} className="mr-1.5 min-w-0 flex-[1_1_220px] font-medium hover:underline">
            {task.title ?? task.id}
          </TaskLink>
          <Decision task={task} small onDone={() => undefined} />
        </div>
      ))}
    </section>
  );
}

/** Where the docs and the code disagree. Left out while the design loads or when the project has none. */
function Findings() {
  const graph = useArchGraph();
  if (!graph.data) return null;
  const drift = graph.data.drift.slice(0, FINDINGS);
  return (
    <section className="flex flex-col">
      <h2 className={HEADING}>Something looks wrong</h2>
      {drift.length === 0 ? <p className="text-sm text-muted-foreground">Docs and code agree.</p> : null}
      {drift.map((finding) => (
        <AppLink key={`${finding.path}:${finding.line}:${finding.message}`} href="/architecture" className={`${ROW} hover:underline`}>
          {finding.message}
        </AppLink>
      ))}
    </section>
  );
}

function Milestones({ milestones, byMilestone }: { milestones: ListRow[]; byMilestone: Map<string, ListRow[]> }) {
  const open = milestones.filter((row) => row.status !== "done");
  return (
    <section className="flex flex-col">
      <h2 className={HEADING}>Milestones</h2>
      {open.map((row) => {
        const ref = milestoneRef(row);
        const { done, total } = progress(byMilestone.get(refKey(ref)) ?? []);
        const percent = total === 0 ? 0 : Math.round((done / total) * 100);
        return (
          <AppLink key={row.id} href={`/milestones/${ref}`} className="grid grid-cols-[minmax(0,1fr)_90px_44px] items-center gap-3 border-t py-[7px] hover:underline">
            <span>{row.title ?? row.id}</span>
            <span className="h-1.5 rounded-[3px] bg-muted">
              <span className="block h-1.5 rounded-[3px] bg-st-done" style={{ width: `${percent}%` }} />
            </span>
            <span className="text-right font-mono text-xs">{percent}%</span>
          </AppLink>
        );
      })}
    </section>
  );
}

/** The home page: what needs the owner now, without opening the board. */
export function NowPage() {
  // The same list and grouping as the Milestones page, so a milestone reads the same percent here.
  const work = useWork();
  const rows = work.tasks;
  const count = (status: string) => rows.filter((row) => row.status === status).length;
  const review = rows.filter((row) => row.status === "review");
  return (
    <div>
      <PageHeader title="Now" />
      {work.isPending ? <Loading lines={4} /> : null}
      {work.error ? <ErrorNotice error={work.error} /> : null}
      {!work.isPending && !work.error ? (
        <div className="flex flex-col gap-5 text-sm leading-[1.4]">
          <FirstRun tasks={rows} milestones={work.milestones} />
          <div className="grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(170px,1fr))]">
            <Tile href="/board" count={review.length} label="waiting for your review" alarm />
            <Tile href="/board" count={count("in-progress")} label="in progress" />
            <Tile href="/board" count={count("todo")} label="ready to start" />
            <Tile href="/milestones" count={rows.filter((row) => !row.milestone && !isOver(row.status)).length} label="open with no milestone" alarm />
          </div>
          <div className="grid items-start gap-6 [grid-template-columns:repeat(auto-fit,minmax(340px,1fr))]">
            <Decisions review={review} />
            <div className="flex min-w-0 flex-col gap-5">
              <Findings />
              <Milestones milestones={work.milestones} byMilestone={work.byMilestone} />
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
