import { createFileRoute } from "@tanstack/react-router";
import { PlanPage } from "@/components/planning/PlanPage";

export const Route = createFileRoute("/plans/$")({
  component: PlanRoute,
});

function PlanRoute() {
  const { _splat } = Route.useParams();
  return <PlanPage id={_splat ?? ""} />;
}
