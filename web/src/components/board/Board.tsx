import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ListRow } from "@/api/types/ListRow";
import { useConfig } from "@/api/useConfig";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { useItemRows } from "@/components/markdown/useResolver";
import { TaskLink } from "@/components/peek/TaskLink";
import { More } from "@/components/ui/page";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import {
  type BoardFilters,
  type Column,
  ageOf,
  boardListParams,
  columnLabel,
  groupByStatus,
  isTucked,
  matchesFilters,
  openSummary,
  servicesOf,
  shownStatuses,
  toggleShown,
} from "./model";

const GRID = "grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] items-start gap-3.5";
const CONTROL = "rounded-[7px] border border-input bg-card px-[11px] py-[5px] text-[13px] text-foreground";

function Card({ row, milestone, today }: { row: ListRow; milestone: string | undefined; today: Date }) {
  const where = [milestone ?? row.milestone, ...(row.services ?? [])].filter((part) => part !== undefined && part !== "").join(" · ");
  const age = ageOf(row.created, today);
  const done = row.criteria_total === undefined ? null : `${row.criteria_done ?? 0}/${row.criteria_total}`;
  return (
    <div aria-label={`${row.short ?? row.id}: ${row.title ?? ""}`} className="slab relative flex flex-col gap-[7px] px-3 py-[11px]">
      <div className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
        {row.short ?? row.id}
        {row.priority === "high" ? (
          <span className="rounded bg-st-blocked-soft px-1.5 font-sans font-semibold text-st-blocked">High</span>
        ) : null}
        {done === null ? null : <span className="ml-auto text-primary">{done}</span>}
      </div>
      <TaskLink row={row} className="text-sm font-[550] after:absolute after:inset-0 hover:underline">
        {row.title ?? row.id}
      </TaskLink>
      {where === "" ? null : <p className="truncate text-[12.5px] text-muted-foreground">{where}</p>}
      {age === null && !row.assignee ? null : (
        <div className="flex justify-between gap-2 border-t border-muted pt-[7px] text-[12.5px] text-muted-foreground">
          <span>{age}</span>
          <span>{row.assignee}</span>
        </div>
      )}
    </div>
  );
}

/** A long column shows this many cards until asked for the rest, so the board stays quick to draw. */
const CARD_LIMIT = 4;

function ColumnView({ column, titleOf, today }: { column: Column; titleOf: (milestone: string | undefined) => string | undefined; today: Date }) {
  const label = columnLabel(column.status);
  return (
    <section aria-label={label} className="flex min-w-0 flex-col gap-2">
      <h2 className="flex items-center gap-2 px-0.5 pt-0.5 pb-1 text-sm font-semibold">
        {label}
        <span className="rounded-[10px] bg-muted px-[7px] font-mono text-xs font-medium">{column.rows.length}</span>
      </h2>
      <More rows={column.rows} limit={CARD_LIMIT} label={(hidden) => `Show ${hidden} more`} className="self-start px-0.5 py-[3px] text-[13px] font-normal">
        {(shown) => shown.map((row) => <Card key={row.id} row={row} milestone={titleOf(row.milestone)} today={today} />)}
      </More>
    </section>
  );
}

function Controls({ filters, services, onFilters }: { filters: BoardFilters; services: string[]; onFilters: (next: BoardFilters) => void }) {
  const { data: rows } = useItemRows();
  const milestones = (rows ?? []).filter((row) => row.type === "milestone");
  const set = (key: keyof BoardFilters, value: string) => onFilters({ ...filters, [key]: value === "" ? undefined : value });
  const high = filters.priority === "high";
  return (
    <div className="ml-auto flex flex-wrap gap-2">
      <input
        type="search"
        aria-label="Filter tasks"
        placeholder="Filter by id or title"
        value={filters.q ?? ""}
        onChange={(event) => set("q", event.target.value)}
        className={cn(CONTROL, "w-48")}
      />
      <select
        aria-label="Milestone"
        value={filters.milestone ?? ""}
        onChange={(event) => set("milestone", event.target.value)}
        className={cn(CONTROL, "w-36 max-w-full truncate")}
      >
        <option value="">All milestones</option>
        {milestones.map((row) => (
          <option key={row.id} value={row.short ?? row.id}>
            {row.short ?? row.id}
            {row.title ? `: ${row.title}` : ""}
          </option>
        ))}
      </select>
      <select aria-label="Service" value={filters.service ?? ""} onChange={(event) => set("service", event.target.value)} className={CONTROL}>
        <option value="">All services</option>
        {services.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
      <button
        type="button"
        aria-pressed={high}
        onClick={() => onFilters({ ...filters, priority: high ? undefined : "high" })}
        className={cn(CONTROL, "aria-pressed:border-primary aria-pressed:text-primary")}
      >
        High priority
      </button>
    </div>
  );
}

/** Tasks in one column per open status; draft, done and archived wait behind a count. Filters live in the URL. */
export function Board({ filters, onFilters }: { filters: BoardFilters; onFilters: (next: BoardFilters) => void }) {
  const config = useConfig();
  const milestoneRows = useItemRows();
  const params = boardListParams(filters);
  const tasks = useQuery({ queryKey: keys.items.list(params), queryFn: () => api.get<ListRow[]>("/api/items", params) });

  const { q, service, priority } = filters;
  const columns = useMemo(
    () =>
      groupByStatus(
        (tasks.data ?? []).filter((row) => matchesFilters(row, { q, service, priority })),
        config.data?.statuses.task ?? [],
        config.data?.priorities ?? [],
      ),
    [tasks.data, config.data, q, service, priority],
  );
  // The chosen service stays listed even when no task carries it any more.
  const services = useMemo(() => {
    const names = new Set(servicesOf(tasks.data ?? []));
    if (service !== undefined) names.add(service);
    return [...names].sort((a, b) => a.localeCompare(b));
  }, [tasks.data, service]);
  const titles = useMemo(() => {
    const out = new Map<string, string>();
    for (const row of milestoneRows.data ?? []) {
      if (row.type !== "milestone" || !row.title) continue;
      out.set(row.id, row.title);
      if (row.short) out.set(row.short, row.title);
    }
    return out;
  }, [milestoneRows.data]);
  const titleOf = (milestone: string | undefined) => (milestone === undefined ? undefined : titles.get(milestone));

  const shown = shownStatuses(filters.show);
  const open = columns.filter((column) => !isTucked(column.status) || shown.includes(column.status));
  const tucked = columns.filter((column) => isTucked(column.status));
  const today = new Date();

  const error = config.error ?? tasks.error;
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1.5">
        <h1 className="text-2xl leading-8 font-[650]">Board</h1>
        <p className="text-sm text-muted-foreground">{openSummary(columns)}</p>
        <Controls filters={filters} services={services} onFilters={onFilters} />
      </div>
      {error ? <ErrorNotice error={error} /> : null}
      {tasks.isPending || config.isPending ? (
        <div aria-busy="true" className={GRID}>
          <span className="sr-only">Loading...</span>
          {[6, 4, 2].map((cards) => (
            <div key={cards} className="space-y-2">
              <Skeleton className="h-4 w-20" />
              {Array.from({ length: cards }, (_, at) => (
                <Skeleton key={at} className="h-24 w-full" />
              ))}
            </div>
          ))}
        </div>
      ) : null}
      <div className={GRID}>
        {open.map((column) => (
          <ColumnView key={column.status} column={column} titleOf={titleOf} today={today} />
        ))}
      </div>
      {tucked.length === 0 ? null : (
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-[13px] text-muted-foreground">
          {tucked.map((column) => (
            <button
              key={column.status}
              type="button"
              aria-pressed={shown.includes(column.status)}
              onClick={() => onFilters({ ...filters, show: toggleShown(filters.show, column.status) })}
              className="rounded hover:text-foreground hover:underline aria-pressed:font-semibold aria-pressed:text-primary"
            >
              {columnLabel(column.status)} {column.rows.length}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
