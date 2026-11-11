import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { FeatureDetail } from "@/api/types/FeatureDetail";
import type { Product } from "@/api/types/Product";

/** The overview and the features. A project with neither answers empty, not with an error. */
export function useProduct() {
  return useQuery({ queryKey: keys.product(), queryFn: () => api.get<Product>("/api/product") });
}

/** One feature with what it connects to, by its doc id. */
export function featureQuery(id: string) {
  return {
    queryKey: keys.feature(id),
    queryFn: () => api.get<FeatureDetail>(`/api/features/${id.split("/").map(encodeURIComponent).join("/")}`),
  };
}
