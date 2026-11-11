import "@xyflow/react/dist/style.css";
import { Background, Controls, Panel, ReactFlow } from "@xyflow/react";
import { X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { Resolver } from "@/components/markdown/links";
import { internalClick } from "@/components/markdown/nav";
import { useIsDark } from "@/components/markdown/useIsDark";
import { C4Edge } from "./C4Edge";
import { C4Group, C4Leaf, C4Name } from "./C4Nodes";
import { type C4Geometry, drawC4, LABELS_ALL_UP_TO, type LabelMode, placeC4, relText } from "./c4Layout";
import { type C4Diagram, type C4Element, kindLabel, parseC4 } from "./parse";
import { useNarrow } from "./useNarrow";

const NODE_TYPES = { c4: C4Leaf, c4Group: C4Group, c4Name: C4Name };
const EDGE_TYPES = { c4Edge: C4Edge };

export interface C4ViewProps {
  source: string;
  resolver: Resolver;
  onNavigate?: ((href: string) => void) | undefined;
  /** Tailwind height of the drawing; the Architecture page fills the screen, a doc keeps a box. */
  className?: string;
  /** Where a box linked to this item opens its inside view; a double-click goes there. */
  viewHref?: ((id: string) => string | undefined) | undefined;
  /** Every arrow shows its label, whatever their number; the choice between all and selected goes away. */
  allLabels?: boolean;
  /** The drawing's box takes the drawing's own proportions, so no empty area is left; `className` then only caps its height. */
  fit?: boolean;
}

function Heading({ children }: { children: string }) {
  return <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{children}</h3>;
}

function ItemLink({
  target,
  resolver,
  onNavigate,
  viewHref,
}: {
  target: string;
  resolver: Resolver;
  onNavigate: C4ViewProps["onNavigate"];
  viewHref: C4ViewProps["viewHref"];
}) {
  const found = resolver.wiki(target);
  if (found === null) {
    return resolver.ready ? (
      <p className="text-sm text-destructive" title={`${target} was not found`}>
        Links to {target}, which does not exist
      </p>
    ) : null;
  }
  const href = found.href;
  const view = viewHref?.(found.id);
  return (
    <div className="space-y-1">
      {view === undefined ? null : (
        <a
          href={view}
          onClick={internalClick(onNavigate, view)}
          className="block rounded-md border px-3 py-1 text-center text-sm font-medium hover:bg-accent"
        >
          Open the inside view
        </a>
      )}
      <a href={href} onClick={internalClick(onNavigate, href)} className="block text-sm text-primary underline underline-offset-2">
        Open {found.id}
      </a>
    </div>
  );
}

function Details({
  diagram,
  element,
  resolver,
  onNavigate,
  viewHref,
  onSelect,
  onClose,
}: {
  diagram: C4Diagram;
  element: C4Element;
  resolver: Resolver;
  onNavigate: C4ViewProps["onNavigate"];
  viewHref: C4ViewProps["viewHref"];
  onSelect: (alias: string) => void;
  onClose: () => void;
}) {
  const label = new Map(diagram.elements.map((e) => [e.alias, e.label]));
  const out = diagram.rels.filter((rel) => rel.from === element.alias);
  const into = diagram.rels.filter((rel) => rel.to === element.alias);
  const children = diagram.elements.filter((e) => e.parent === element.alias);
  const Other = ({ alias }: { alias: string }) => (
    <button type="button" onClick={() => onSelect(alias)} className="font-medium text-primary hover:underline">
      {label.get(alias) ?? alias}
    </button>
  );
  return (
    <aside
      aria-label={`${element.label} details`}
      className="absolute inset-y-2 right-2 z-10 w-80 max-w-[85%] space-y-4 overflow-y-auto rounded-md border bg-card p-4 text-card-foreground shadow-lg"
    >
      <header className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{element.type ?? kindLabel(element)}</p>
          <h2 className="text-base font-semibold">{element.label}</h2>
          {element.technology === undefined ? null : <p className="text-xs text-muted-foreground">{element.technology}</p>}
        </div>
        <button type="button" onClick={onClose} aria-label="Close details" className="rounded p-1 text-muted-foreground hover:bg-accent">
          <X className="size-4" />
        </button>
      </header>
      {element.description === undefined ? null : <p className="text-sm leading-6">{element.description}</p>}
      {element.link === undefined ? null : (
        <ItemLink target={element.link} resolver={resolver} onNavigate={onNavigate} viewHref={viewHref} />
      )}
      {out.length + into.length === 0 ? null : (
        <section aria-label="Relationships">
          <Heading>Relationships</Heading>
          <ul className="space-y-2 text-sm">
            {out.map((rel, at) => (
              <li key={`out${at}`}>
                <span aria-hidden="true">→ </span>
                <Other alias={rel.to} />: {relText(rel)}
                {rel.description === undefined ? null : <p className="text-xs text-muted-foreground">{rel.description}</p>}
              </li>
            ))}
            {into.map((rel, at) => (
              <li key={`in${at}`}>
                <span aria-hidden="true">← </span>
                <Other alias={rel.from} />: {relText(rel)}
                {rel.description === undefined ? null : <p className="text-xs text-muted-foreground">{rel.description}</p>}
              </li>
            ))}
          </ul>
        </section>
      )}
      {children.length === 0 ? null : (
        <section aria-label="Inside">
          <Heading>Inside</Heading>
          <ul className="space-y-1 text-sm">
            {children.map((child) => (
              <li key={child.alias}>
                <Other alias={child.alias} />
              </li>
            ))}
          </ul>
        </section>
      )}
    </aside>
  );
}

/** A Mermaid C4 diagram drawn as nested boxes with labelled arrows; a click opens an element's details. */
export function C4View({ source, resolver, onNavigate, className, viewHref, allLabels = false, fit = false }: C4ViewProps) {
  const dark = useIsDark();
  const phone = useNarrow() && fit;
  const parsed = useMemo(() => parseC4(source), [source]);
  const [selected, setSelected] = useState<string | null>(null);
  const diagram = "diagram" in parsed ? parsed.diagram : null;
  const [labelChoice, setLabels] = useState<LabelMode | null>(null);
  const labels: LabelMode = allLabels ? "all" : (labelChoice ?? ((diagram?.rels.length ?? 0) > LABELS_ALL_UP_TO ? "selected" : "all"));
  const [geometry, setGeometry] = useState<{ diagram: C4Diagram; reserved: boolean; geometry: C4Geometry } | { error: string } | null>(null);
  const reserved = labels === "all";
  useEffect(() => {
    if (diagram === null) return;
    let live = true;
    placeC4(diagram, reserved).then(
      (placed) => live && setGeometry({ diagram, reserved, geometry: placed }),
      (error: unknown) =>
        live && setGeometry({ error: `Laying it out failed: ${error instanceof Error ? error.message : String(error)}` }),
    );
    return () => {
      live = false;
    };
  }, [diagram, reserved]);
  const current = geometry !== null && "diagram" in geometry && geometry.diagram === diagram && geometry.reserved === reserved ? geometry.geometry : null;
  // With a `viewHref` only a box that goes somewhere shows the link icon; without one a linked box opens its details.
  const opens = useCallback(
    (element: C4Element) => {
      if (element.link === undefined) return false;
      if (viewHref === undefined) return true;
      const found = resolver.wiki(element.link);
      return found !== null && viewHref(found.id) !== undefined;
    },
    [resolver, viewHref],
  );
  const placed = useMemo(
    () => (diagram === null || current === null ? null : drawC4(diagram, current, selected, labels, opens)),
    [diagram, current, selected, labels, opens],
  );

  const failed = "error" in parsed ? parsed.error : geometry !== null && "error" in geometry ? geometry.error : null;
  if (failed !== null || diagram === null) {
    return (
      <div role="alert" className="my-4 rounded-md border border-destructive/50 p-3 text-sm">
        <p className="font-medium text-destructive">The diagram does not render</p>
        <p className="mt-1">{failed}</p>
      </div>
    );
  }
  const element = diagram.elements.find((e) => e.alias === selected);
  return (
    <figure aria-label={diagram.title ?? "C4 diagram"} className="my-4 space-y-2">
      <div
        className={`relative w-full rounded-md border ${className ?? (phone ? "h-[calc(100dvh-22rem)] min-h-[300px]" : fit ? "max-h-[78vh] min-h-[300px]" : "h-[560px]")}`}
        {...(fit && !phone && current !== null && current.height > 0 ? { style: { aspectRatio: `${current.width} / ${current.height}` } } : {})}
      >
        {diagram.elements.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">This diagram has no elements yet.</p>
        ) : placed === null ? (
          <p className="p-4 text-sm text-muted-foreground">Drawing…</p>
        ) : (
          <ReactFlow
            key={phone ? "phone" : "wide"}
            nodes={placed.nodes.map((node) => ({ ...node, selected: node.id === selected }))}
            edges={placed.edges}
            nodeTypes={NODE_TYPES}
            edgeTypes={EDGE_TYPES}
            colorMode={dark ? "dark" : "light"}
            nodesDraggable={false}
            nodesConnectable={false}
            // On a phone a fitted drawing is too small to read: open at a readable zoom on the top left, and the fit button shows it all.
            {...(phone ? { defaultViewport: { x: 12, y: 12, zoom: 0.7 } } : { fitView: true })}
            fitViewOptions={{ padding: 0.03, maxZoom: 1.5 }}
            minZoom={0.1}
            onNodeClick={(_, node) => setSelected(node.id)}
            onNodeDoubleClick={(_, node) => {
              const link = diagram.elements.find((e) => e.alias === node.id)?.link;
              const found = link === undefined ? null : resolver.wiki(link);
              const view = found === null ? undefined : viewHref?.(found.id);
              if (view !== undefined) onNavigate?.(view);
            }}
            onPaneClick={() => setSelected(null)}
          >
            <Background />
            <Controls showInteractive={false} />
            {allLabels ? null : (
              <Panel position="top-right">
                <button
                  type="button"
                  onClick={() => setLabels(labels === "all" ? "selected" : "all")}
                  aria-pressed={labels === "all"}
                  className="rounded-md border bg-card px-2 py-1 text-xs text-card-foreground shadow-sm hover:bg-accent"
                >
                  {labels === "all" ? "Labels on every arrow" : "Labels on the selection's arrows"}
                </button>
              </Panel>
            )}
          </ReactFlow>
        )}
        {element === undefined ? null : (
          <Details
            diagram={diagram}
            element={element}
            resolver={resolver}
            onNavigate={onNavigate}
            viewHref={viewHref}
            onSelect={setSelected}
            onClose={() => setSelected(null)}
          />
        )}
      </div>
      {diagram.skipped.length === 0 ? null : (
        <details className="text-xs text-muted-foreground">
          <summary>
            {diagram.skipped.length} {diagram.skipped.length === 1 ? "line was" : "lines were"} not drawn
          </summary>
          <ul className="mt-1 space-y-0.5 font-mono">
            {diagram.skipped.map((skip) => (
              <li key={`${skip.line}:${skip.text}`}>
                {skip.line}: {skip.text}
              </li>
            ))}
          </ul>
        </details>
      )}
    </figure>
  );
}
