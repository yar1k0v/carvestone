import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Board } from "@/components/board/Board";
import type { BoardFilters } from "@/components/board/model";
import { useWidePage } from "@/shell/pageFrame";

function text(value: unknown): string | undefined {
  return typeof value === "string" && value !== "" ? value : undefined;
}

export const Route = createFileRoute("/board")({
  validateSearch: (search: Record<string, unknown>): BoardFilters => ({
    milestone: text(search.milestone),
    service: text(search.service),
    priority: search.priority === "high" ? "high" : undefined,
    q: text(search.q),
    show: text(search.show),
  }),
  component: BoardRoute,
});

function BoardRoute() {
  useWidePage();
  const filters = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return <Board filters={filters} onFilters={(next) => void navigate({ search: next, replace: true })} />;
}
