import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { useRowsOfType } from "@/components/planning/useRows";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { StatusBadge } from "@/components/ui/status";

export type ShelfName = "decisions" | "plans";

const SHELVES: Record<ShelfName, { type: string; title: string; description: string; empty: string }> = {
  decisions: { type: "decision", title: "Decisions", description: "Why the system is the way it is", empty: "No decisions yet." },
  plans: { type: "plan", title: "Plans", description: "What we set out to do, and how", empty: "No plans yet." },
};

/** Every decision or plan on its own shelf, each with its status. */
export function Shelf({ name }: { name: ShelfName }) {
  const shelf = SHELVES[name];
  const rows = useRowsOfType(shelf.type);
  return (
    <div className="space-y-5 pb-8">
      <PageHeader title={shelf.title} description={shelf.description} className="mb-0" />
      {rows.isPending ? <Loading lines={6} label={`Reading the ${shelf.title.toLowerCase()}...`} /> : null}
      {rows.isError ? <ErrorNotice error={rows.error} /> : null}
      {rows.data?.length === 0 ? <p className="text-sm text-muted-foreground">{shelf.empty}</p> : null}
      {rows.data === undefined || rows.data.length === 0 ? null : (
        <ul className="slab divide-y overflow-hidden">
          {rows.data.map((row) => (
            <li key={row.id} className="flex items-center gap-3 px-3 py-2 text-sm transition-colors hover:bg-accent/50">
              <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground">{row.short ?? row.id}</span>
              <AppLink href={hrefFor(row.id)} className="min-w-0 flex-1 truncate font-medium hover:underline">
                {row.title ?? row.id}
              </AppLink>
              <StatusBadge status={row.status} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
