import type { QueryClient } from "@tanstack/react-query";
import { api } from "./client";
import { keys } from "./keys";
import type { GetResult } from "./types/GetResult";

function item(client: QueryClient, id: string) {
  const path = `/api/items/${id.split("/").map(encodeURIComponent).join("/")}`;
  void client.prefetchQuery({ queryKey: keys.items.get(id), queryFn: () => api.get<GetResult>(path) });
}

/** The page behind an in-app link starts loading when the pointer rests on the link. */
export function prefetchHref(client: QueryClient, href: string): void {
  const path = decodeURI((href.split(/[?#]/)[0] ?? href).replace(/^\/+/, ""));
  const [root, ...rest] = path.split("/");
  const tail = rest.join("/");
  if (tail === "") return;
  if (root === "items") item(client, tail);
  else if (root === "plans" || root === "milestones" || root === "docs" || root === "tasks") item(client, `${root}/${tail}`);
  else if (root === "features") {
    const id = `docs/${tail}`;
    void client.prefetchQuery({ queryKey: keys.feature(id), queryFn: () => api.get(`/api/features/${id}`), retry: false });
  }
  else if (root === "architecture") {
    // The class and module pages name their subject in the query, so there is no folder to explain.
    if (tail === "class" || tail === "module") return;
    // A container opens on its written design; the code's view of it waits until asked for.
    if (tail.startsWith("arch/")) {
      void client.prefetchQuery({ queryKey: keys.lld(tail), queryFn: () => api.get(`/api/arch/lld/${tail}`, { service: false }), retry: false });
      return;
    }
    const params = { target: tail };
    void client.prefetchQuery({ queryKey: keys.code("explain", params), queryFn: () => api.get("/api/code/explain", params) });
  }
}
