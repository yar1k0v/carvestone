import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { SearchPage, type SearchPageParams } from "@/components/search/SearchPage";

function text(value: unknown): string | undefined {
  return typeof value === "string" && value !== "" ? value : undefined;
}

export const Route = createFileRoute("/search")({
  // The top bar and the palette send the typed query here.
  validateSearch: (search: Record<string, unknown>): SearchPageParams => ({
    q: text(search.q),
    type: text(search.type),
    status: text(search.status),
    milestone: text(search.milestone),
    component: text(search.component),
  }),
  component: SearchRoute,
});

function SearchRoute() {
  const params = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return <SearchPage params={params} onParams={(next) => void navigate({ search: next, replace: true })} />;
}
