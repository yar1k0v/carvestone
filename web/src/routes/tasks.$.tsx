import { createFileRoute } from "@tanstack/react-router";
import { TaskPage } from "@/components/work/TaskPage";

export const Route = createFileRoute("/tasks/$")({
  component: TaskRoute,
});

function TaskRoute() {
  const { _splat } = Route.useParams();
  const target = _splat ?? "";
  return <TaskPage key={target} target={target} />;
}
