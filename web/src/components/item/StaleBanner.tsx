import type { Stale } from "@/api/types/Stale";

/** Shown above an item whose code or review date has moved on. */
export function StaleBanner({ stale }: { stale?: Stale | null }) {
  // The server leaves `stale` out of a fresh item, so it is undefined as often as null.
  if (!stale) return null;
  return (
    <div
      role="alert"
      className="mb-4 rounded-md border border-amber-500/50 bg-amber-500/10 px-3 py-2 text-sm text-amber-900 dark:text-amber-200"
    >
      <span className="font-medium">Stale.</span> {stale.reason}
    </div>
  );
}
