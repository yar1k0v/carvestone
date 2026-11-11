import type { Trust } from "@/api/types/Trust";

const TONE: Record<Trust, string> = {
  unverified: "border-border text-muted-foreground",
  "machine-confirmed": "border-sky-500/50 text-sky-700 dark:text-sky-300",
  "human-reviewed": "border-emerald-500/50 text-emerald-700 dark:text-emerald-300",
};

/** The engine's trust value, shown as it is. */
export function TrustBadge({ trust }: { trust: Trust }) {
  return (
    <span data-trust={trust} className={`rounded-full border px-2 py-0.5 text-xs font-medium ${TONE[trust]}`}>
      {trust}
    </span>
  );
}
