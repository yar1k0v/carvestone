import { useLocation } from "@tanstack/react-router";
import { AppLink } from "@/components/markdown/AppLink";
import { cn } from "@/lib/utils";
import { isActive, type NavEntry } from "./nav";

/** The entries of one space; the same on a wide screen and in the phone drawer. */
export function SpaceLinks({ entries }: { entries: NavEntry[] }) {
  const pathname = useLocation({ select: (location) => location.pathname });
  const search = useLocation({ select: (location) => location.searchStr });
  return (
    <nav aria-label="Main" className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-3 py-4">
      {entries.map((entry) => {
        const active = isActive(pathname, entry, search);
        return (
          <AppLink
            key={entry.href}
            href={entry.href}
            aria-current={active ? "page" : undefined}
            className={cn(
              "flex items-baseline justify-between gap-2 rounded-md px-2.5 py-[7px] text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground",
              active && "bg-accent font-semibold text-accent-foreground",
            )}
          >
            <span className="truncate">{entry.label}</span>
            {entry.count === undefined ? null : <span className="shrink-0 font-mono text-xs font-normal">{entry.count}</span>}
          </AppLink>
        );
      })}
    </nav>
  );
}

/** The column beside the page; a space with no entries has none. */
export function Sidebar({ entries }: { entries: NavEntry[] }) {
  if (entries.length === 0) return null;
  return (
    <aside className="hidden w-[208px] shrink-0 flex-col border-r bg-sidebar md:flex">
      <SpaceLinks entries={entries} />
    </aside>
  );
}
