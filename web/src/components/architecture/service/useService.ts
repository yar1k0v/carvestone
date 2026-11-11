import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ArchGraph } from "@/api/types/ArchGraph";
import type { DomainModel } from "@/api/types/DomainModel";
import type { Event } from "@/api/types/Event";
import type { Explain } from "@/api/types/Explain";
import { useCode } from "@/components/graph/useCode";
import { lldQuery } from "@/components/graph/useDesign";

const COMMITS_SHOWN = 8;

/** Everything the store and the code say about the elements. */
export function useArchGraph() {
  return useQuery({ queryKey: keys.arch(), queryFn: () => api.get<ArchGraph>("/api/arch/graph") });
}

export function useExplain(id: string) {
  return useCode<Explain>("explain", { target: id });
}

/** The entities of one service, or of the whole project. */
export function useDomain(part?: string) {
  return useCode<DomainModel>("domain", part === undefined ? {} : { part });
}

/** The docs written about a service. Anything that is not a container has none, which is no error. */
export function useWritten(id: string) {
  return useQuery(lldQuery(id));
}

export function useCommits(component: string) {
  const params = { component, kind: "commit", limit: COMMITS_SHOWN };
  return useQuery({ queryKey: keys.timeline(params), queryFn: () => api.get<Event[]>("/api/timeline", params) });
}
