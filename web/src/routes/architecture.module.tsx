import { createFileRoute, redirect } from "@tanstack/react-router";
import { ModulePage } from "@/components/architecture/symbol/ModulePage";
import { searchText } from "@/components/architecture/symbol/href";

export const Route = createFileRoute("/architecture/module")({
  validateSearch: (search: Record<string, unknown>): { id?: string } => ({ id: searchText(search.id) }),
  beforeLoad: ({ search }) => {
    if (search.id === undefined) throw redirect({ to: "/architecture", replace: true });
  },
  component: ModuleRoute,
});

function ModuleRoute() {
  const { id = "" } = Route.useSearch();
  return <ModulePage key={id} id={id} />;
}
