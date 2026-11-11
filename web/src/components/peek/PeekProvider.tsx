import { useLocation } from "@tanstack/react-router";
import { type ReactNode, Suspense, lazy, useCallback, useState } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { PeekContext } from "./peek";

// The task view renders markdown, so it loads with the first peek and not with the shell.
const TaskOverlay = lazy(() => import("./TaskOverlay"));

/** Holds which task the overlay shows and on which page; on any other page it is closed. */
export function PeekProvider({ children }: { children: ReactNode }) {
  const pathname = useLocation({ select: (location) => location.pathname });
  const [peek, setPeek] = useState<{ row: ListRow; on: string } | null>(null);
  const open = useCallback((row: ListRow) => setPeek({ row, on: pathname }), [pathname]);
  const close = useCallback(() => setPeek(null), []);
  return (
    <PeekContext.Provider value={open}>
      {children}
      {peek === null || peek.on !== pathname ? null : (
        <Suspense fallback={null}>
          <TaskOverlay key={peek.row.id} row={peek.row} onClose={close} />
        </Suspense>
      )}
    </PeekContext.Provider>
  );
}
