import { createFileRoute } from "@tanstack/react-router";
import { ProductPage } from "@/components/product/ProductPage";

export const Route = createFileRoute("/product")({
  component: ProductPage,
});
