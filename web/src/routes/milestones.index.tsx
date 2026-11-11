import { createFileRoute } from "@tanstack/react-router";
import { MilestoneList } from "@/components/planning/MilestoneList";

export const Route = createFileRoute("/milestones/")({
  component: MilestoneList,
});
