import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/milestones")({
  component: Outlet,
});
