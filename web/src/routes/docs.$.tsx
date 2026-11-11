import { createFileRoute } from "@tanstack/react-router";
import { DocPage } from "@/components/markdown/DocPage";

export const Route = createFileRoute("/docs/$")({
  component: DocRoute,
});

function DocRoute() {
  const { _splat } = Route.useParams();
  return <DocPage docId={`docs/${_splat ?? ""}`} />;
}
