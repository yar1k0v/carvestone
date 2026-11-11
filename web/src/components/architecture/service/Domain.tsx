import { useEffect, useMemo, useRef } from "react";
import type { DomainModel } from "@/api/types/DomainModel";
import type { Entity } from "@/api/types/Entity";
import type { EntityRelation } from "@/api/types/EntityRelation";
import { CARD_H, CARD_W, type Box, edgePath, layoutParts } from "@/components/graph/hldLayout";
import { AppLink } from "@/components/markdown/AppLink";
import { classHref } from "@/components/architecture/symbol/href";
import { More } from "@/components/ui/page";
import { cn } from "@/lib/utils";
import { centredLayout, fieldText, relationLines } from "./domainLayout";
import { Answer, CodeLink, Empty, Note, link } from "./parts";
import { useDomain } from "./useService";

/** Past this many related entities a drawing stops helping; the list carries them. */
const DRAW_UP_TO = 60;
const FIELDS_ON_CARD = 3;
/** Room right of the last column for the loops that join two cards of one column; only a centred drawing has them. */
const LOOP_ROOM = 72;
/** The padding of the frame (`p-4`), which the drawing starts inside. */
const FRAME_PAD = 16;

/** Halfway along the line between the facing sides of two cards, as `edgePath` draws it. */
function labelAt(from: Box, to: Box): { x: number; y: number } {
  const apart = to.x !== from.x;
  const sx = from.x + (to.x < from.x ? 0 : CARD_W);
  const tx = to.x + (to.x > from.x ? 0 : CARD_W);
  // Two cards in one column are joined by a loop out of the right side, so the label sits on the loop.
  const x = apart ? (sx + tx) / 2 : sx + 42;
  return { x, y: (from.y + to.y + CARD_H) / 2 };
}

const CARD = "slab absolute overflow-hidden px-3 py-2 text-left";

/**
 * The entities and how they relate. With `onPick` each card is a button that picks its entity,
 * and the `centre` card is marked.
 */
export function Diagram({
  entities,
  relations,
  centre,
  onPick,
}: {
  entities: Entity[];
  relations: EntityRelation[];
  centre?: string;
  onPick?: (name: string) => void;
}) {
  const byName = useMemo(() => new Map(entities.map((entity) => [entity.name, entity])), [entities]);
  const layout = useMemo(() => {
    const related = new Set(relations.flatMap((relation) => [relation.from, relation.to]));
    const names = entities.map((entity) => entity.name).filter((name) => related.has(name));
    return centre === undefined ? layoutParts(names, relations) : centredLayout(names, relations, centre);
  }, [entities, relations, centre]);
  const at = new Map(layout.boxes.map((box) => [box.id, box]));
  const frame = useRef<HTMLDivElement>(null);
  // The frame scrolls both ways on a narrow screen or a busy centre; start with the centre card in view.
  useEffect(() => {
    const box = layout.boxes.find((each) => each.id === centre);
    if (frame.current === null || box === undefined) return;
    frame.current.scrollLeft = FRAME_PAD + box.x + CARD_W / 2 - frame.current.clientWidth / 2;
    frame.current.scrollTop = FRAME_PAD + box.y + CARD_H / 2 - frame.current.clientHeight / 2;
  }, [centre, layout]);
  const drawn = relations.filter((relation) => relation.from !== relation.to && at.has(relation.from) && at.has(relation.to));
  return (
    // A button inside an image would be hidden from assistive tools, so a drawing that picks is a group.
    <div
      ref={frame}
      role={onPick === undefined ? "img" : "group"}
      aria-label="Domain model diagram"
      className="max-h-[36rem] min-w-0 overflow-auto rounded-lg border bg-muted/40 p-4"
    >
      <div className="relative" style={{ width: layout.width + (centre === undefined ? 0 : LOOP_ROOM), height: layout.height }}>
        <svg aria-hidden className="pointer-events-none absolute inset-0 overflow-visible" width={layout.width} height={layout.height}>
          <defs>
            <marker id="domain-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 0.5 L 7 4 L 0 7.5 z" className="fill-muted-foreground" />
            </marker>
          </defs>
          {drawn.map((relation) => (
            <path
              key={`${relation.from}>${relation.to}:${relation.label ?? ""}`}
              d={edgePath(at.get(relation.from)!, at.get(relation.to)!)}
              fill="none"
              strokeWidth={1.1}
              markerEnd="url(#domain-arrow)"
              className="stroke-muted-foreground/55"
            />
          ))}
        </svg>
        {drawn.map((relation, index) => {
          if (relation.label === undefined) return null;
          const mid = labelAt(at.get(relation.from)!, at.get(relation.to)!);
          // Two relations between one pair share a line, so their labels stack instead of overlapping.
          const pair = drawn.filter((other) => [other.from, other.to].sort().join() === [relation.from, relation.to].sort().join());
          const shift = (pair.indexOf(relation) - (pair.length - 1) / 2) * 16;
          return (
            <span
              key={`label:${relation.from}>${relation.to}:${relation.label}:${index}`}
              style={{ left: mid.x, top: mid.y + shift }}
              className="pointer-events-none absolute -translate-x-1/2 -translate-y-1/2 rounded bg-muted px-1 text-xs whitespace-nowrap text-muted-foreground"
            >
              {relation.label}
            </span>
          );
        })}
        {layout.boxes.map((box) => {
          const entity = byName.get(box.id);
          if (entity === undefined) return null;
          const style = { left: box.x, top: box.y, width: CARD_W, height: CARD_H };
          const inside = (
            <>
              <span className="block truncate text-sm font-semibold">{entity.name}</span>
              <span className="block font-mono text-[0.6875rem] leading-4 text-muted-foreground">
                {entity.fields.slice(0, FIELDS_ON_CARD).map((field) => (
                  <span key={field.name} className="block truncate">
                    {fieldText(field)}
                  </span>
                ))}
              </span>
            </>
          );
          if (onPick === undefined) {
            return (
              <div key={box.id} style={style} className={CARD}>
                {inside}
              </div>
            );
          }
          return (
            <button
              key={box.id}
              type="button"
              aria-label={entity.name}
              aria-current={entity.name === centre ? "true" : undefined}
              onClick={() => onPick(entity.name)}
              style={style}
              className={cn(CARD, "cursor-pointer hover:border-primary", entity.name === centre && "border-2 border-primary bg-[color-mix(in_srgb,var(--primary)_10%,var(--card))]")}
            >
              {inside}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Row({ entity, relations }: { entity: Entity; relations: EntityRelation[] }) {
  const out = relationLines(entity, relations);
  return (
    <li className="px-3 py-2 text-sm">
      <p className="flex flex-wrap items-baseline justify-between gap-x-3">
        <span className="flex flex-wrap items-baseline gap-x-2">
          <AppLink href={classHref(entity.symbol)} className={`font-medium ${link}`}>
            {entity.name}
          </AppLink>
        </span>
        {entity.file === undefined ? null : <CodeLink file={entity.file} line={entity.line} />}
      </p>
      {entity.fields.length === 0 ? null : (
        <p className="font-mono text-xs break-words text-muted-foreground">
          {entity.fields.map(fieldText).join(", ")}
          {(entity.more_fields ?? 0) > 0 ? `, and ${entity.more_fields} more` : ""}
        </p>
      )}
      {out.length === 0 ? null : (
        <p className="text-xs text-muted-foreground">{out.join(", ")}</p>
      )}
    </li>
  );
}

function Model({ model }: { model: DomainModel }) {
  const { entities, relations, note } = model;
  const related = new Set(relations.flatMap((relation) => [relation.from, relation.to])).size;
  return (
    <>
      <Note>{note}</Note>
      {related > 0 && related <= DRAW_UP_TO ? <div className="mb-4">
          <Diagram entities={entities} relations={relations} />
        </div> : null}
      {related === 0 ? <p className="mb-2 text-sm text-muted-foreground">No relations between these entities were found.</p> : null}
      {related > DRAW_UP_TO ? <p className="mb-2 text-sm text-muted-foreground">Too many relations to draw; the list holds them all.</p> : null}
      <More rows={entities} limit={12}>
        {(shown) => (
          <ul aria-label="Entities" className="slab divide-y">
            {shown.map((entity) => (
              <Row key={entity.symbol} entity={entity} relations={relations} />
            ))}
          </ul>
        )}
      </More>
    </>
  );
}

/** A service with no entities of its own works on the project's: most keep them in a shared library. */
function ProjectModel() {
  const query = useDomain();
  return (
    <Answer query={query} label="Reading the domain model...">
      {(model) =>
        model.entities.length === 0 ? (
          <Empty>No entities found in this project's code.</Empty>
        ) : (
          <>
            <p className="mb-2 text-sm text-muted-foreground">No entities sit in this service's own code. This is the model of the whole project.</p>
            <Model model={model} />
          </>
        )
      }
    </Answer>
  );
}

/** The entities this service's code holds and how they relate, drawn and listed. */
export function Domain({ part }: { part: string }) {
  const query = useDomain(part);
  return (
    <Answer query={query} label="Reading the domain model...">
      {(model) => (model.entities.length === 0 ? <ProjectModel /> : <Model model={model} />)}
    </Answer>
  );
}
