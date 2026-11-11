import { createFileRoute } from "@tanstack/react-router";
import { MilestonePage } from "@/components/planning/MilestonePage";

export const Route = createFileRoute("/milestones/$")({
  component: MilestoneRoute,
});

function MilestoneRoute() {
  const { _splat } = Route.useParams();
  return <MilestonePage id={_splat ?? ""} />;
}
