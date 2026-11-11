import { cn } from "@/lib/utils";

/** One grey bar that stands in for text still on its way. */
export function Skeleton({ className }: { className?: string }) {
  return <span aria-hidden className={cn("skeleton block h-4", className)} />;
}

/** A block of lines; screen readers hear `label`. */
export function Loading({ lines = 3, label = "Loading...", className }: { lines?: number; label?: string; className?: string }) {
  return (
    <div className={cn("space-y-2.5", className)} aria-busy="true">
      <span className="sr-only">{label}</span>
      {Array.from({ length: lines }, (_, at) => (
        <Skeleton key={at} className={at === lines - 1 ? "w-3/5" : at % 2 === 0 ? "w-full" : "w-11/12"} />
      ))}
    </div>
  );
}

/** Rows of a list still on its way. */
export function LoadingRows({ rows = 5, label = "Loading..." }: { rows?: number; label?: string }) {
  return (
    <div className="slab divide-y" aria-busy="true">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, at) => (
        <div key={at} className="flex items-center gap-4 px-4 py-3.5">
          <Skeleton className="h-3 w-12" />
          <Skeleton className={at % 2 === 0 ? "w-1/2" : "w-1/3"} />
          <Skeleton className="ml-auto h-3 w-16" />
        </div>
      ))}
    </div>
  );
}
