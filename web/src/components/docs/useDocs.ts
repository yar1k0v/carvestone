import { keepPreviousData, useQueries, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { DocRow } from "@/api/types/DocRow";
import type { GetResult } from "@/api/types/GetResult";
import type { Hit } from "@/api/types/Hit";
import type { SearchResult } from "@/api/types/SearchResult";
import { useHld } from "@/components/graph/useDesign";
import { fetchItem, isItemView } from "@/components/item/useItem";
import { DEBOUNCE_MS, useDebounced } from "@/components/search/useSearch";

/** Sections asked for per search, as many as the engine ranks: several can belong to one doc, and the list sorts by date. */
const SECTIONS = 1000;

/** Every doc with what a reader needs to choose one, the most recently changed first. `enabled` false keeps a page outside the Library from reading them. */
export function useDocRows(enabled = true) {
  return useQuery({ queryKey: keys.docs(), queryFn: () => api.get<DocRow[]>("/api/docs"), enabled });
}

/** A part of the product and the docs linked to it. */
export interface DocPart {
  id: string;
  title: string;
  docs: Set<string>;
}

/** The services of the design with the docs that link to each; empty for a project with no design. */
export function useDocParts(): DocPart[] {
  const containers = useHld().data?.containers;
  const views = useQueries({
    queries: (containers ?? []).map((container) => ({
      queryKey: keys.items.get(container.id),
      queryFn: () => fetchItem(container.id),
    })),
    combine: (results): (GetResult | undefined)[] => results.map((result) => result.data),
  });
  return useMemo(
    () =>
      (containers ?? []).map((container, at) => {
        const view = views[at];
        const backlinks = view !== undefined && isItemView(view) ? view.backlinks : [];
        const docs = new Set(backlinks.map((link) => link.id).filter((id) => id.startsWith("docs/")));
        return { id: container.id, title: container.title, docs };
      }),
    [containers, views],
  );
}

export interface DocTextSearch {
  /** The best hit of each doc that holds the text, by doc id. */
  hits: Map<string, Hit>;
  /** The text the hits answer. */
  query: string;
  busy: boolean;
  failed: boolean;
  /** More sections match than were asked for. */
  cut: boolean;
}

/** Searches inside the docs once the text has stood still. Text with no word to look for sends nothing. */
export function useDocTextSearch(text: string): DocTextSearch {
  const typed = text.trim();
  const settled = useDebounced(typed, DEBOUNCE_MS);
  const searchable = /[\p{L}\p{N}]/u.test(settled);
  const params = { q: settled, type: "doc", limit: SECTIONS };
  const query = useQuery({
    queryKey: keys.search(params),
    queryFn: () => api.get<SearchResult>("/api/search", params),
    enabled: searchable,
    placeholderData: keepPreviousData,
    retry: false,
  });
  const result = typed !== "" && searchable ? query.data : undefined;
  const hits = useMemo(() => {
    const best = new Map<string, Hit>();
    for (const hit of result?.hits ?? []) if (!best.has(hit.id)) best.set(hit.id, hit);
    return best;
  }, [result]);
  return {
    hits,
    query: result?.query ?? settled,
    busy: typed !== settled || (searchable && query.isFetching),
    failed: searchable && query.isError,
    cut: result !== undefined && (result.truncated || result.hits.length >= SECTIONS),
  };
}
