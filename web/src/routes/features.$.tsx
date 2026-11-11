import { createFileRoute } from "@tanstack/react-router";
import { FeaturePage } from "@/components/product/FeaturePage";
import { type FeatureTab, tabOf } from "@/components/product/model";
import { useWidePage } from "@/shell/pageFrame";

export const Route = createFileRoute("/features/$")({
  validateSearch: (search: Record<string, unknown>): { tab?: FeatureTab } => {
    const tab = tabOf(search.tab);
    return tab === undefined ? {} : { tab };
  },
  component: FeatureRoute,
});

function FeatureRoute() {
  const { _splat } = Route.useParams();
  // Checked again here so that an unknown value always opens the first tab.
  const tab = tabOf(Route.useSearch().tab);
  const navigate = Route.useNavigate();
  useWidePage();
  const target = _splat ?? "";
  return <FeaturePage key={target} target={target} tab={tab} onTab={(next) => void navigate({ search: { tab: next }, replace: true })} />;
}
