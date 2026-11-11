import { createFileRoute, redirect } from "@tanstack/react-router";
import { ClassPage } from "@/components/architecture/symbol/ClassPage";
import { searchText } from "@/components/architecture/symbol/href";

export const Route = createFileRoute("/architecture/class")({
  validateSearch: (search: Record<string, unknown>): { target?: string } => ({ target: searchText(search.target) }),
  beforeLoad: ({ search }) => {
    if (search.target === undefined) throw redirect({ to: "/architecture", replace: true });
  },
  component: ClassRoute,
});

function ClassRoute() {
  const { target = "" } = Route.useSearch();
  // A new target is a new page: nothing of the last one's state carries over.
  return <ClassPage key={target} target={target} />;
}
