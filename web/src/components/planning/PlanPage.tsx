import { useMemo } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { useConfig } from "@/api/useConfig";
import { dropTitle } from "@/components/docs/lead";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { isItemView, useCachedRow, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { hrefFor } from "@/components/markdown/links";
import { useGo, useItemRows, useResolver } from "@/components/markdown/useResolver";
import { Loading } from "@/components/ui/skeleton";
import { StackedBar, tally } from "@/components/ui/status";
import { StatusBadge, TaskGroups } from "./TaskRows";
import { textField } from "./progress";

/** Rows for ids, keeping the order and dropping repeats; an id with no row gets a bare one. */
function rowsFor(ids: string[], byId: Map<string, ListRow>): ListRow[] {
  return [...new Set(ids)].map(
    (id) => byId.get(id) ?? { id, short: null, path: "", type: "", status: null, priority: null, title: null, etag: "", changed_files: [] },
  );
}

export function PlanPage({ id }: { id: string }) {
  const item = useItemQuery(`plans/${id}`);
  const cached = useCachedRow(`plans/${id}`);
  const rows = useItemRows();
  const config = useConfig();
  const resolver = useResolver();
  const go = useGo();
  const byId = useMemo(() => new Map((rows.data ?? []).map((row) => [row.id, row])), [rows.data]);

  if (item.isPending) {
    return (
      <article className="space-y-6">
        <header>
          <p className="eyebrow">plans/{id}</p>
          <h1 className="text-2xl font-semibold">{cached?.title ?? id}</h1>
          <div className="mt-2">
            <StatusBadge status={cached?.status ?? null} />
          </div>
        </header>
        <Loading lines={8} className="max-w-3xl" />
      </article>
    );
  }
  if (item.isError) return <ErrorNotice error={item.error} />;
  if (!isItemView(item.data)) return <ErrorNotice error={new Error(`${id} is not a plan`)} />;

  const view = item.data;
  // A task belongs to the plan when its `plan` field names it, which the index records as a `plan` backlink.
  const tasks = rowsFor(
    view.backlinks.filter((link) => link.kind === "plan" && link.id.startsWith("tasks/")).map((link) => link.id),
    byId,
  );
  const linked = Array.isArray(view.fields.links) ? view.fields.links.filter((v): v is string => typeof v === "string") : [];
  const decisionIds = [
    ...view.backlinks.filter((link) => link.id.startsWith("decisions/")).map((link) => link.id),
    ...linked.map((target) => resolver.wiki(target)?.id).filter((v): v is string => v !== undefined && v.startsWith("decisions/")),
  ];
  const decisions = rowsFor(decisionIds, byId);
  const milestone = textField(view.fields, "milestone");
  const milestoneLink = milestone === null ? null : resolver.wiki(milestone);
  const t = tally(tasks.map((task) => task.status));

  return (
    <article className="space-y-6">
      <header>
        <p className="eyebrow">{view.id}</p>
        <h1 className="text-2xl font-semibold">{textField(view.fields, "title") ?? view.id}</h1>
        <div className="mt-2 flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
          <StatusBadge status={textField(view.fields, "status")} />
          {milestoneLink ? (
            <AppLink href={`/milestones/${milestoneLink.id.replace(/^milestones\//, "")}`} className="underline underline-offset-2">
              {milestone}
            </AppLink>
          ) : (
            <span>{milestone ?? "no milestone"}</span>
          )}
          <AppLink href={`/items/${view.id}`} className="underline underline-offset-2">
            Open as item
          </AppLink>
        </div>
        {t.total === 0 ? null : (
          <div className="mt-4 flex max-w-xl items-center gap-3 text-xs text-muted-foreground">
            <StackedBar tally={t} label="Plan progress" className="h-2 flex-1" />
            <span className="tabular-nums">
              {t.done}/{t.total} done
            </span>
          </div>
        )}
      </header>
      <div className="flex flex-col gap-8 xl:flex-row">
        <div className="min-w-0 max-w-3xl flex-1">
          <Markdown source={dropTitle(view.body, textField(view.fields, "title") ?? "")} resolver={resolver} fromId={view.id} onNavigate={go} />
        </div>
        <aside className="w-full shrink-0 space-y-6 xl:w-96">
          <section>
            <h2 className="mb-2 text-lg font-semibold">Tasks ({tasks.length})</h2>
            {tasks.length === 0 ? <p className="text-sm text-muted-foreground">No tasks.</p> : <TaskGroups tasks={tasks} statuses={config.data?.statuses.task} />}
          </section>
          <section>
            <h2 className="mb-2 text-lg font-semibold">Decisions</h2>
            {decisions.length === 0 ? (
              <p className="text-sm text-muted-foreground">No linked decisions.</p>
            ) : (
              <ul className="space-y-1 text-sm">
                {decisions.map((row) => (
                  <li key={row.id} className="flex items-center gap-2">
                    <AppLink href={hrefFor(row.id)} className="underline underline-offset-2">
                      {row.short ?? row.id}
                    </AppLink>
                    <span className="truncate text-muted-foreground">{row.title}</span>
                    <StatusBadge status={row.status} />
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>
      </div>
    </article>
  );
}
