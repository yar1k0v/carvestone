import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { ActivityPage } from "@/components/work/ActivityPage";
import { type ActivitySearch, isPeriod } from "@/components/work/activityModel";

export const Route = createFileRoute("/activity")({
  validateSearch: (search: Record<string, unknown>): ActivitySearch => ({
    period: isPeriod(search.period) ? search.period : undefined,
    from: typeof search.from === "string" && search.from !== "" ? search.from : undefined,
  }),
  component: ActivityRoute,
});

function ActivityRoute() {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return <ActivityPage search={search} onSearch={(next) => void navigate({ search: next, replace: true })} />;
}
