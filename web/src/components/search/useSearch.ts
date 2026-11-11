import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { Hit } from "@/api/types/Hit";
import type { SearchResult } from "@/api/types/SearchResult";
import { useDocRows } from "@/components/docs/useDocs";
import { useItemRows } from "@/components/markdown/useResolver";
import { groupHits } from "./hits";

export const DEBOUNCE_MS = 200;
/** The cost cap per answer, in the engine's units (characters over four). */
export const SEARCH_BUDGET = 400;

export interface SearchFilters {
  type?: string;
  status?: string;
  milestone?: string;
  component?: string;
}

/** `value` once it has stood still for `ms`. */
export function useDebounced<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

/** The hits under their group names, with titles from the item and doc lists the app already holds. */
export function useGroupedHits(hits: Hit[] | undefined) {
  const rows = useItemRows().data;
  const docs = useDocRows().data;
  return useMemo(() => groupHits(hits ?? [], rows ?? [], docs ?? []), [hits, rows, docs]);
}

/** Searches items and code after the text has stood still for 200 ms; blank text sends nothing. */
export function useSearch(text: string, filters: SearchFilters = {}) {
  const typed = text.trim();
  const settled = useDebounced(typed, DEBOUNCE_MS);
  const params = { q: settled, budget: SEARCH_BUDGET, ...filters };
  const query = useQuery({
    queryKey: keys.search(params),
    queryFn: () => api.get<SearchResult>("/api/search", params),
    enabled: settled !== "",
    placeholderData: keepPreviousData,
  });
  return {
    result: settled === "" ? undefined : query.data,
    error: query.error,
    /** True while the text is waiting out the debounce or the answer is on its way. */
    busy: typed !== settled || (settled !== "" && query.isFetching),
    query: settled,
  };
}
