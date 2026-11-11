import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { CodeView } from "@/api/types/CodeView";
import type { GetResult } from "@/api/types/GetResult";
import type { ItemView } from "@/api/types/ItemView";
import type { ListRow } from "@/api/types/ListRow";

export function isItemView(result: GetResult): result is ItemView {
  return "body" in result;
}

export function isCodeView(result: GetResult): result is CodeView {
  return "lines" in result;
}

/** `tasks/CS-2` becomes `/api/items/tasks/CS-2`; each segment is encoded, the slashes are not. */
export function itemUrl(id: string): string {
  return `/api/items/${id.split("/").map(encodeURIComponent).join("/")}`;
}

export function fetchItem(id: string): Promise<GetResult> {
  return api.get<GetResult>(itemUrl(id));
}

/** The item for an id as a route spells it: a path id or a short id. */
export function useItemQuery(id: string, enabled = true) {
  return useQuery({ queryKey: keys.items.get(id), queryFn: () => fetchItem(id), enabled });
}

/** The row any list already fetched for this id, so a page can name itself before its item arrives. */
export function useCachedRow(id: string): ListRow | null {
  const client = useQueryClient();
  return useMemo(() => {
    for (const [, rows] of client.getQueriesData<ListRow[]>({ queryKey: ["items", "list"] })) {
      const row = rows?.find((candidate) => candidate.id === id || candidate.short === id);
      if (row) return row;
    }
    return null;
  }, [client, id]);
}
