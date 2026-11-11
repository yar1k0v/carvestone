import { createFileRoute, redirect } from "@tanstack/react-router";
import { searchText } from "@/components/architecture/symbol/href";

/** Item chips and symbol hits link here as `/code?target=<name>`: that opens the class page. */
export const Route = createFileRoute("/code")({
  validateSearch: (search: Record<string, unknown>): { target?: string } => ({ target: searchText(search.target) }),
  beforeLoad: ({ search }) => {
    if (search.target === undefined) throw redirect({ to: "/architecture", replace: true });
    throw redirect({ to: "/architecture/class", search: { target: search.target }, replace: true });
  },
});
