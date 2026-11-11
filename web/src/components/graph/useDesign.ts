import { useQueries, useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { Hld } from "@/api/types/Hld";
import type { Lld } from "@/api/types/Lld";

/** The high-level design (`stone arch hld`). */
export function useHld() {
  return useQuery({ queryKey: keys.hld(), queryFn: () => api.get<Hld>("/api/arch/hld") });
}

/** The written low-level design of one container; an error for anything else. */
export function lldQuery(container: string) {
  return {
    queryKey: keys.lld(container),
    // Only the docs: the code's own view is asked for when someone opens it.
    queryFn: () => api.get<Lld>(`/api/arch/lld/${container}`, { service: false }),
    retry: false,
  };
}

/** The written design of each container, by id, for the list and the page beside it. */
export function useContainerDocs(containers: { id: string }[]) {
  const answers = useQueries({ queries: containers.map((part) => lldQuery(part.id)) });
  return new Map(containers.map((part, at) => [part.id, answers[at]]));
}
