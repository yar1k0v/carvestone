import { useQuery } from "@tanstack/react-query";
import { api } from "./client";
import { keys } from "./keys";
import type { ApiConfig } from "./types/ApiConfig";

/** Statuses, priorities and field specs from the server; they change only with the repo config. */
export function useConfig() {
  return useQuery({ queryKey: keys.config(), queryFn: () => api.get<ApiConfig>("/api/config"), staleTime: Infinity });
}
