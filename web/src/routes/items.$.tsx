import { createFileRoute } from "@tanstack/react-router";
import { ItemPage } from "@/components/item/ItemPage";

export const Route = createFileRoute("/items/$")({
  component: ItemRoute,
});

function ItemRoute() {
  const { _splat } = Route.useParams();
  return <ItemPage id={_splat ?? ""} />;
}
