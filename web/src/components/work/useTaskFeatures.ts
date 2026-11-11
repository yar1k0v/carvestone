import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { TaskFeature } from "@/api/types/TaskFeature";
import { useProduct } from "@/components/product/useProduct";

export interface TaskFeatures {
  rows: TaskFeature[];
  isPending: boolean;
  /** The project lists no features at all. */
  none: boolean;
}

/** The features of a task, from one answer: only the server knows what a task shares with a feature. */
export function useTaskFeatures(id: string): TaskFeatures {
  const product = useProduct();
  const short = id.replace(/^tasks\//, "");
  const query = useQuery({
    queryKey: keys.taskFeatures(short),
    queryFn: () => api.get<TaskFeature[]>(`/api/tasks/${encodeURIComponent(short)}/features`),
  });
  return { rows: query.data ?? [], isPending: query.isPending, none: product.data?.features.length === 0 };
}
