import { createContext, useContext } from "react";
import type { ListRow } from "@/api/types/ListRow";

/** Opens the task view over the page on a row; `null` outside the shell, where a link simply navigates. */
export const PeekContext = createContext<((row: ListRow) => void) | null>(null);

export function usePeek() {
  return useContext(PeekContext);
}
