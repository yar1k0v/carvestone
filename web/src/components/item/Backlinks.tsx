import type { Backlink } from "@/api/types/Backlink";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";

/** What links here: the linking item, the kind of link and its line. */
export function Backlinks({ items }: { items: Backlink[] }) {
  return (
    <section aria-label="Backlinks" className="text-sm">
      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Backlinks</h2>
      {items.length === 0 ? (
        <p className="text-muted-foreground">Nothing links here.</p>
      ) : (
        <ul className="space-y-1">
          {items.map((link) => (
            <li key={`${link.id}#${link.anchor ?? ""}:${link.line}:${link.kind}`} className="flex flex-wrap gap-x-2">
              <AppLink href={hrefFor(link.id, link.anchor)} className="text-primary underline underline-offset-2">
                {link.id}
                {link.anchor ? `#${link.anchor}` : ""}
              </AppLink>
              <span className="text-muted-foreground">
                {link.kind}, line {link.line}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
