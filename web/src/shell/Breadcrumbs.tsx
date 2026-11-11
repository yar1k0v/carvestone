import { useLocation } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { useMemo } from "react";
import { useHld } from "@/components/graph/useDesign";
import { AppLink } from "@/components/markdown/AppLink";
import { useItemRows } from "@/components/markdown/useResolver";
import { crumbsFor } from "./crumbs";
import type { Crumb } from "./pageFrame";

/** Titles by path id and short id, from the item list and the design that are already cached. */
function useTitles(): (id: string) => string | undefined {
  const { data: rows } = useItemRows();
  const hld = useHld();
  return useMemo(() => {
    const titles = new Map<string, string>();
    for (const row of rows ?? []) {
      if (row.title === null) continue;
      titles.set(row.id, row.title);
      if (row.short !== null) titles.set(row.short, row.title);
    }
    for (const part of [...(hld.data?.systems ?? []), ...(hld.data?.containers ?? [])]) titles.set(part.id, part.title);
    return (id) => titles.get(id);
  }, [rows, hld.data]);
}

/** Where the page sits: the page's own trail when it gave one, otherwise worked out from the address. */
export function Breadcrumbs({ override }: { override: Crumb[] | null }) {
  const pathname = useLocation({ select: (location) => location.pathname });
  const titleOf = useTitles();
  const crumbs = override ?? crumbsFor(pathname, titleOf);
  return (
    <nav aria-label="Breadcrumb" className="mb-2.5">
      <ol className="flex items-center gap-1 text-sm text-muted-foreground">
        {crumbs.map((crumb, at) => (
          <li key={`${crumb.label}:${at}`} className="flex min-w-0 items-center gap-1">
            {at === 0 ? null : <ChevronRight aria-hidden className="size-3.5 shrink-0" />}
            {crumb.href === undefined ? (
              <span aria-current={at === crumbs.length - 1 ? "page" : undefined} className="truncate">
                {crumb.label}
              </span>
            ) : (
              <AppLink href={crumb.href} className="truncate hover:text-foreground">
                {crumb.label}
              </AppLink>
            )}
          </li>
        ))}
      </ol>
    </nav>
  );
}
