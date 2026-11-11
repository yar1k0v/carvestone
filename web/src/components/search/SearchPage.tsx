import { useConfig } from "@/api/useConfig";
import { useItemRows } from "@/components/markdown/useResolver";
import { cn } from "@/lib/utils";
import { SearchHits } from "./SearchHits";
import { type SearchFilters, useGroupedHits, useSearch } from "./useSearch";

/** What the URL keeps for the full page: the text and the four filters. */
export interface SearchPageParams extends SearchFilters {
  q?: string;
}

const TYPES = ["doc", "plan", "task", "milestone", "decision", "reference", "system", "container", "component", "symbol"];

const FIELD = "h-9 rounded-md border border-input bg-transparent px-2 text-sm";

export function SearchPage({ params, onParams }: { params: SearchPageParams; onParams: (next: SearchPageParams) => void }) {
  const { q = "", ...filters } = params;
  const { result, query, busy, error } = useSearch(q, filters);
  const groups = useGroupedHits(result?.hits);
  const config = useConfig();
  const { data: rows } = useItemRows();
  const statuses = [...new Set(Object.values(config.data?.statuses ?? {}).flat())];
  const milestones = (rows ?? []).filter((row) => row.type === "milestone");
  const components = (rows ?? []).filter((row) => row.type === "component");
  const set = (key: keyof SearchPageParams, value: string) => onParams({ ...params, [key]: value === "" ? undefined : value });

  return (
    <div className="max-w-3xl">
      <h1 className="mb-4 text-2xl font-semibold tracking-tight">Search</h1>
      <input
        type="search"
        aria-label="Search query"
        placeholder="Search items and code"
        value={q}
        autoFocus
        onChange={(event) => set("q", event.target.value)}
        className={cn(FIELD, "h-10 w-full")}
      />
      <div className="mt-2 flex flex-wrap gap-2">
        <select aria-label="Type" value={filters.type ?? ""} onChange={(event) => set("type", event.target.value)} className={FIELD}>
          <option value="">All types</option>
          {TYPES.map((type) => (
            <option key={type}>{type}</option>
          ))}
        </select>
        <select aria-label="Status" value={filters.status ?? ""} onChange={(event) => set("status", event.target.value)} className={FIELD}>
          <option value="">Any status</option>
          {statuses.map((status) => (
            <option key={status}>{status}</option>
          ))}
        </select>
        <select aria-label="Milestone" value={filters.milestone ?? ""} onChange={(event) => set("milestone", event.target.value)} className={FIELD}>
          <option value="">Any milestone</option>
          {milestones.map((row) => (
            <option key={row.id} value={row.short ?? row.id}>
              {row.short ?? row.id}
            </option>
          ))}
        </select>
        <select aria-label="Component" value={filters.component ?? ""} onChange={(event) => set("component", event.target.value)} className={FIELD}>
          <option value="">Any component</option>
          {components.map((row) => (
            <option key={row.id} value={row.id}>
              {row.title ?? row.id}
            </option>
          ))}
        </select>
      </div>
      <div className="mt-4">
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error.message}
          </p>
        ) : null}
        {result ? <SearchHits groups={groups} query={query} /> : null}
        {result?.truncated ? <p className="px-3 pt-2 text-xs text-muted-foreground">More results exist; narrow the query or add a filter.</p> : null}
        {!result && busy ? <p className="text-sm text-muted-foreground">Searching...</p> : null}
      </div>
    </div>
  );
}
