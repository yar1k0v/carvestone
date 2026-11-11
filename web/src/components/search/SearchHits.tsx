import { AppLink } from "@/components/markdown/AppLink";
import { cn } from "@/lib/utils";
import { type HitGroup, highlight, hitHref, type ShownHit } from "./hits";

function HitRow({ shown, query, active, compact }: { shown: ShownHit; query: string; active: boolean; compact: boolean }) {
  const { hit, title, sub } = shown;
  return (
    <li>
      <AppLink
        href={hitHref(hit)}
        aria-current={active ? "true" : undefined}
        className={cn("block rounded-md px-3 py-2 text-sm hover:bg-accent", active && "bg-accent")}
      >
        <span className="block truncate font-[550]">{title}</span>
        <span className="block truncate text-[13px] text-muted-foreground">{sub}</span>
        {compact ? null : (
          <span className="line-clamp-2 text-muted-foreground">
            {highlight(hit.snippet, query).map((piece, at) =>
              piece.match ? (
                <mark key={at} className="rounded-sm bg-amber-200/70 text-foreground dark:bg-amber-500/30">
                  {piece.text}
                </mark>
              ) : (
                <span key={at}>{piece.text}</span>
              ),
            )}
          </span>
        )}
      </AppLink>
    </li>
  );
}

/** Hits under their group names; `active` counts rows across the groups and marks the one Enter would open. */
export function SearchHits({ groups, query, active, compact = false }: { groups: HitGroup[]; query: string; active?: number; compact?: boolean }) {
  if (groups.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-sm text-muted-foreground">
        No results for &ldquo;{query}&rdquo;. Try fewer or broader words, or clear the filters.
      </p>
    );
  }
  let at = 0;
  return (
    <>
      {groups.map((group) => (
        <section key={group.name} className="mb-2">
          <h2 className="px-3 pt-2 pb-1 text-xs font-semibold text-muted-foreground">{group.name}</h2>
          <ul>
            {group.hits.map((shown) => {
              const row = at++;
              const { hit } = shown;
              return (
                <HitRow
                  key={`${hit.id}#${hit.anchor ?? ""}:${hit.path}:${hit.lines[0]}`}
                  shown={shown}
                  query={query}
                  active={row === active}
                  compact={compact}
                />
              );
            })}
          </ul>
        </section>
      ))}
    </>
  );
}
