import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { Module } from "@/api/types/Module";
import type { SymbolPage } from "@/api/types/SymbolPage";
import type { Why } from "@/api/types/Why";
import { useCode } from "@/components/graph/useCode";
import { useHld } from "@/components/graph/useDesign";
import { useItemRows } from "@/components/markdown/useResolver";

/** All a class page shows, in one answer. Takes a name, a short name, a path or `path:line`. */
export function useSymbol(target: string) {
  return useCode<SymbolPage>("symbol", { target });
}

/** Why a file or line exists: its service, commits and the items they name. */
export function useWhy(path: string | undefined, line?: number) {
  const params = { path: path ?? "", line };
  return useQuery({
    queryKey: keys.code("why", params),
    queryFn: () => api.get<Why>("/api/why", params),
    enabled: path !== undefined,
  });
}

export function useModules() {
  return useCode<Module[]>("modules", {});
}

/** A title for an element id from what the shell has already fetched; the id's last part until then. */
export function useElementTitle(id: string | null): string | null {
  const { data: rows } = useItemRows();
  const hld = useHld();
  if (id === null) return null;
  return hld.data?.containers.find((part) => part.id === id)?.title ?? rows?.find((row) => row.id === id)?.title ?? id.split("/").at(-1) ?? id;
}
