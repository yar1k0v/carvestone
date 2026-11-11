import { useIsFetching } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useEffect, useState } from "react";

/** Shorter work than this never shows the bar, so a fast page does not flicker. */
const DELAY_MS = 180;

/** A thin line along the top edge while anything is on its way from the server. */
export function TopProgress() {
  const fetching = useIsFetching();
  const routing = useRouterState({ select: (state) => state.status === "pending" });
  const busy = fetching > 0 || routing;
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setShown(busy), busy ? DELAY_MS : 0);
    return () => clearTimeout(timer);
  }, [busy]);

  if (!shown) return null;
  return (
    <div aria-hidden className="fixed inset-x-0 top-0 z-[60] h-0.5 overflow-hidden bg-primary/15">
      <div className="h-full w-full origin-left bg-primary motion-safe:animate-[creep_1.2s_ease-in-out_infinite]" />
    </div>
  );
}
