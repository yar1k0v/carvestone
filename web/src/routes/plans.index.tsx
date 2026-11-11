import { createFileRoute } from "@tanstack/react-router";
import { PlanList } from "@/components/planning/PlanList";

export const Route = createFileRoute("/plans/")({
  component: PlanList,
});
