import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ListRow } from "@/api/types/ListRow";

/** Every item of one type, in the order the server lists them. */
export function useRowsOfType(type: string) {
  const params = { type };
  return useQuery({ queryKey: keys.items.list(params), queryFn: () => api.get<ListRow[]>("/api/items", params) });
}

/** The tasks of one milestone (`M-2` or its path id). */
export function useMilestoneTasks(milestone: string) {
  const params = { type: "task", milestone };
  return useQuery({ queryKey: keys.items.list(params), queryFn: () => api.get<ListRow[]>("/api/items", params) });
}
