import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { C4Source } from "@/api/types/C4Source";

/** The context diagram (`stone arch c4 context`). */
export function useC4Context() {
  return useQuery({ queryKey: keys.c4Context(), queryFn: () => api.get<C4Source>("/api/arch/c4/context"), retry: false });
}

/** The container diagram of one system; without `system` the server draws the store's only system. */
export function useC4Container(system?: string) {
  return useQuery({
    queryKey: keys.c4Container(system),
    queryFn: () => api.get<C4Source>("/api/arch/c4/container", { system }),
    retry: false,
  });
}

/** The component diagram of one container. */
export function useC4Component(container: string) {
  return useQuery({
    queryKey: keys.c4Component(container),
    queryFn: () => api.get<C4Source>(`/api/arch/c4/component/${container}`),
    retry: false,
  });
}
