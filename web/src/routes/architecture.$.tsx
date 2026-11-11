import { createFileRoute, redirect } from "@tanstack/react-router";
import { ServicePage } from "@/components/architecture/service/ServicePage";
import { type ServiceTab, serviceTabOf } from "@/components/architecture/service/model";
import { useWidePage } from "@/shell/pageFrame";

export const Route = createFileRoute("/architecture/$")({
  validateSearch: (search: Record<string, unknown>): { tab?: ServiceTab } => {
    const tab = serviceTabOf(search.tab);
    return tab === undefined ? {} : { tab };
  },
  // Anything that is not an element is a folder, and a folder reads as a module.
  beforeLoad: ({ params }) => {
    const target = params._splat ?? "";
    if (!target.startsWith("arch/")) throw redirect({ to: "/architecture/module", search: { id: target }, replace: true });
  },
  component: ServiceRoute,
});

function ServiceRoute() {
  const { _splat } = Route.useParams();
  // Checked again here so that an unknown value always opens the first tab.
  const tab = serviceTabOf(Route.useSearch().tab);
  const navigate = Route.useNavigate();
  useWidePage();
  const target = _splat ?? "";
  // A new target is a new page: nothing of the last one's state carries over.
  return <ServicePage key={target} target={target} tab={tab} onTab={(next) => void navigate({ search: { tab: next }, replace: true })} />;
}
