import { useMemo, useState } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { Chips, PageHeader } from "@/components/ui/page";
import { LoadingRows } from "@/components/ui/skeleton";
import { toneRank } from "@/components/ui/status";
import { useWork } from "./useWork";

/** The id a plan route uses: `m2-search` for `plans/m2-search`. */
function planRef(row: ListRow): string {
  return row.id.replace(/^plans\//, "");
}

function PlanRow({ row }: { row: ListRow }) {
  return (
    <AppLink href={`/plans/${planRef(row)}`} className="flex flex-wrap items-baseline gap-x-4 gap-y-1 px-4 py-3 transition-colors hover:bg-accent/50">
      <span className="min-w-0 flex-[1_1_360px] font-medium">{row.title ?? row.id}</span>
      <span className="w-[92px] text-right font-mono text-xs text-muted-foreground">{row.created}</span>
    </AppLink>
  );
}

/** Every plan of one status, newest first, out of the one cached item list. */
export function PlanList() {
  const work = useWork();
  const [chosen, setChosen] = useState<string | null>(null);

  const statuses = useMemo(() => {
    const counts = new Map<string, number>();
    for (const row of work.plans) counts.set(row.status ?? "none", (counts.get(row.status ?? "none") ?? 0) + 1);
    return [...counts].sort(([a], [b]) => toneRank(a) - toneRank(b));
  }, [work.plans]);

  // Held as null until the owner chooses, so the first view follows the data as it arrives. A choice
  // that no plan has any more gives way to the same rule, so a chip is always selected.
  const first = statuses.some(([value]) => value === "active") ? "active" : (statuses[0]?.[0] ?? "");
  const status = chosen !== null && statuses.some(([value]) => value === chosen) ? chosen : first;

  const shown = useMemo(
    () =>
      work.plans
        .filter((row) => (row.status ?? "none") === status)
        // Newest first; a plan with no day compares as "" so it goes last, in list order (the sort is stable).
        .sort((a, b) => (b.created ?? "").localeCompare(a.created ?? "")),
    [work.plans, status],
  );

  return (
    <div className="text-sm leading-[1.4]">
      <PageHeader title="Plans" />
      {work.isPending ? <LoadingRows /> : null}
      {work.error ? <ErrorNotice error={work.error} /> : null}
      {!work.isPending && !work.error && work.plans.length === 0 ? <p className="text-muted-foreground">No plans yet.</p> : null}
      {work.plans.length > 0 ? (
        <>
          <div className="mb-3.5">
            <Chips label="Plan status" value={status} onChange={setChosen} options={statuses.map(([value, count]) => ({ value, label: value, count }))} />
          </div>
          <div className="slab flex flex-col divide-y overflow-hidden">
            {shown.map((row) => (
              <PlanRow key={row.id} row={row} />
            ))}
          </div>
        </>
      ) : null}
    </div>
  );
}
