import { QueryClient } from "@tanstack/react-query";
import { createMemoryHistory, createRouter, type RouterHistory } from "@tanstack/react-router";
import { ApiError } from "@/api/client";
import { routeTree } from "./routeTree.gen";

/** Retries a failed read twice, but never an answer the server meant (4xx). */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // The event stream invalidates what changed, so a cached answer stays good for a while and
        // coming back to the tab refetches nothing.
        staleTime: 60_000,
        gcTime: 15 * 60_000,
        refetchOnWindowFocus: false,
        retry: (count, error) => !(error instanceof ApiError && error.status >= 400 && error.status < 500) && count < 2,
      },
    },
  });
}

export function createAppRouter(queryClient: QueryClient, history?: RouterHistory) {
  return createRouter({ routeTree, context: { queryClient }, history, defaultPreload: "intent" });
}

/** A router that keeps its location in memory, for tests. */
export function createMemoryRouter(queryClient: QueryClient, path: string) {
  return createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
