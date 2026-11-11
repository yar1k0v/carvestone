import { useMemo, useState } from "react";
import type { Connection } from "@/api/types/Connection";
import type { Part } from "@/api/types/Part";
import { AppLink } from "@/components/markdown/AppLink";
import { cn } from "@/lib/utils";
import { CARD_H, CARD_W, edgePath, layoutParts } from "./hldLayout";
import { serviceHref } from "./serviceHref";

function Card({ part, parent, dim, lit, onHover }: { part: Part; parent?: string; dim: boolean; lit: boolean; onHover: (id: string | null) => void }) {
  return (
    <div
      onMouseEnter={() => onHover(part.id)}
      onMouseLeave={() => onHover(null)}
      style={{ width: CARD_W, height: CARD_H }}
      className={cn(
        "slab group relative flex flex-col gap-1 overflow-hidden px-3 py-2 transition-[opacity,border-color,box-shadow] duration-150",
        dim && "opacity-40",
        lit && "border-primary shadow-md",
      )}
    >
      <AppLink
        href={serviceHref(part.id)}
        className="truncate text-sm font-semibold after:absolute after:inset-0 after:content-[''] group-hover:text-primary"
      >
        {part.title}
      </AppLink>
      {part.description === undefined ? (
        <p className="flex-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{part.id}</p>
      ) : (
        <p className="line-clamp-2 flex-1 text-xs leading-snug text-muted-foreground">{part.description}</p>
      )}
      <div className="flex min-w-0 items-center gap-1.5">
        {part.technology === undefined ? null : (
          <span className="truncate rounded border bg-muted px-1.5 py-px font-mono text-[0.625rem] text-muted-foreground">{part.technology}</span>
        )}
        {parent === undefined ? null : <span className="ml-auto truncate text-[0.625rem] text-muted-foreground">in {parent}</span>}
      </div>
    </div>
  );
}

/**
 * The containers as a drawing: who calls whom left to right. Resting on a card lights its lines and
 * names them; every card opens its low-level design.
 */
export function HldMap({ containers, connections, titles }: { containers: Part[]; connections: Connection[]; titles: Map<string, string> }) {
  const [hover, setHover] = useState<string | null>(null);
  const layout = useMemo(
    () => layoutParts(containers.map((part) => part.id), connections),
    [containers, connections],
  );
  const at = useMemo(() => new Map(layout.boxes.map((box) => [box.id, box])), [layout]);
  const byId = useMemo(() => new Map(containers.map((part) => [part.id, part])), [containers]);

  const lines = connections.flatMap((link) => {
    const from = at.get(link.from);
    const to = at.get(link.to);
    if (from === undefined || to === undefined || link.from === link.to) return [];
    return [{ link, d: edgePath(from, to), mid: { x: (from.x + to.x + CARD_W) / 2, y: (from.y + to.y + CARD_H) / 2 } }];
  });
  // One system over everything says nothing on each card.
  const showParent = new Set(containers.flatMap((part) => (part.parent === undefined ? [] : [part.parent]))).size > 1;
  const touching = (id: string) => connections.some((c) => (c.from === hover && c.to === id) || (c.to === hover && c.from === id));

  return (
    <div className="overflow-x-auto rounded-lg border bg-muted/40 p-5">
      <div className="relative mx-auto" style={{ width: layout.width, height: layout.height }}>
        <svg aria-hidden className="pointer-events-none absolute inset-0 overflow-visible" width={layout.width} height={layout.height}>
          <defs>
            <marker id="hld-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 0.5 L 7 4 L 0 7.5 z" className="fill-muted-foreground" />
            </marker>
            <marker id="hld-arrow-lit" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 0.5 L 7 4 L 0 7.5 z" className="fill-primary" />
            </marker>
          </defs>
          {lines.map(({ link, d }) => {
            const lit = hover !== null && (link.from === hover || link.to === hover);
            return (
              <path
                key={`${link.from}>${link.to}`}
                d={d}
                fill="none"
                strokeWidth={lit ? 1.75 : 1.1}
                markerEnd={lit ? "url(#hld-arrow-lit)" : "url(#hld-arrow)"}
                className={cn(
                  "transition-opacity duration-150",
                  lit ? "stroke-primary" : "stroke-muted-foreground/55",
                  hover !== null && !lit && "opacity-15",
                )}
              />
            );
          })}
        </svg>
        {layout.aloneTop === null ? null : (
          <p className="eyebrow absolute" style={{ top: layout.aloneTop - 22 }}>
            No connections listed
          </p>
        )}
        {layout.boxes.map((box) => {
          const part = byId.get(box.id);
          if (part === undefined) return null;
          const dim = hover !== null && hover !== box.id && !touching(box.id);
          return (
            <div key={box.id} className="absolute" style={{ left: box.x, top: box.y }}>
              <Card
                part={part}
                parent={!showParent || part.parent === undefined ? undefined : (titles.get(part.parent) ?? part.parent)}
                dim={dim}
                lit={hover === box.id}
                onHover={setHover}
              />
            </div>
          );
        })}
        {lines
          .filter(({ link }) => hover !== null && link.description !== undefined && (link.from === hover || link.to === hover))
          .map(({ link, mid }) => (
            <span
              key={`${link.from}>${link.to}:label`}
              aria-hidden
              style={{ left: mid.x, top: mid.y }}
              className="pointer-events-none absolute z-10 max-w-48 -translate-x-1/2 -translate-y-1/2 truncate rounded border bg-popover px-1.5 py-0.5 text-[0.6875rem] shadow-sm"
            >
              {link.description}
            </span>
          ))}
      </div>
    </div>
  );
}
