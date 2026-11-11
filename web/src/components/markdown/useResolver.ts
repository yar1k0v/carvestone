import { useQuery } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import { useCallback, useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ListRow } from "@/api/types/ListRow";
import { type Resolver, buildResolver } from "./links";

/** Every item row; one cached list serves link resolution and the docs sidebar. */
export function useItemRows() {
  return useQuery({ queryKey: keys.items.list(), queryFn: () => api.get<ListRow[]>("/api/items") });
}

/** Resolves `[[...]]` and `.md` links against the item list. */
export function useResolver(): Resolver {
  const { data } = useItemRows();
  return useMemo(() => buildResolver(data ?? [], data !== undefined), [data]);
}

/** Sends an in-app path to the router. */
export function useGo(): (href: string) => void {
  const router = useRouter();
  return useCallback((href: string) => router.history.push(href), [router]);
}
