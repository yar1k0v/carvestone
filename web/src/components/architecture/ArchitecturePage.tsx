import { type ReactNode, useMemo } from "react";
import { ApiError } from "@/api/client";
import type { C4Source } from "@/api/types/C4Source";
import { C4Legend, type C4LegendLevel } from "@/components/c4/C4Legend";
import { C4View } from "@/components/c4/C4View";
import { systemHref, viewHrefs } from "@/components/c4/c4Links";
import { type C4Diagram, parseC4 } from "@/components/c4/parse";
import { useC4Component, useC4Container, useC4Context } from "@/components/graph/useC4";
import { useHld } from "@/components/graph/useDesign";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { useGo, useResolver } from "@/components/markdown/useResolver";
import { Loading } from "@/components/ui/skeleton";

export interface C4PageProps {
  view: "solution" | undefined;
  system: string | undefined;
  container: string | undefined;
}

/** The engine's sentence for a store with nothing to draw. */
const NOTHING_TO_DRAW = /no system to draw/;

function Nothing() {
  return <p className="text-sm text-muted-foreground">Nothing is drawn yet: this store has no system. Run `stone instructions design` to see how to describe one.</p>;
}

/** Loading, the error, or the answer. A store with no system gets one line that says what to run. */
function Answered({ query, children }: { query: { data: C4Source | undefined; error: Error | null; isPending: boolean }; children: (source: string) => ReactNode }) {
  if (query.isPending) return <Loading lines={6} label="Reading the architecture..." className="max-w-3xl" />;
  if (query.error !== null) {
    return query.error instanceof ApiError && NOTHING_TO_DRAW.test(query.error.message) ? <Nothing /> : <ErrorNotice error={query.error} />;
  }
  return query.data === undefined ? null : <>{children(query.data.source)}</>;
}

function Heading({ title, line }: { title: string; line: string }) {
  return (
    <header className="space-y-0.5">
      <h1 className="text-[22px] leading-7 font-semibold">{title}</h1>
      <p className="text-[13px] leading-4 text-muted-foreground">{line}</p>
    </header>
  );
}

/** One generated diagram with its heading, the drawing and the legend under it. */
function Drawn({ source, level, line, container }: { source: string; level: C4LegendLevel; line: (diagram: C4Diagram) => string; container?: (diagram: C4Diagram) => string | undefined }) {
  const resolver = useResolver();
  const go = useGo();
  const parsed = useMemo(() => parseC4(source), [source]);
  const diagram = "diagram" in parsed ? parsed.diagram : null;
  const viewHref = useMemo(() => (diagram === null ? () => undefined : viewHrefs(diagram, resolver)), [diagram, resolver]);
  return (
    <div className="space-y-2">
      {diagram === null ? null : <Heading title={diagram.title ?? "C4 diagram"} line={line(diagram)} />}
      <C4View source={source} resolver={resolver} onNavigate={go} viewHref={viewHref} allLabels fit />
      <C4Legend level={level} container={diagram === null ? undefined : container?.(diagram)} />
    </div>
  );
}

function ContextLevel() {
  const query = useC4Context();
  return (
    <Answered query={query}>
      {(source) => <Drawn source={source} level="context" line={() => "System architecture. C4 level 1. Who uses it and what it talks to."} />}
    </Answered>
  );
}

function ContainerLevel({ system }: { system: string | undefined }) {
  const query = useC4Container(system);
  return (
    <Answered query={query}>
      {(source) => <Drawn source={source} level="container" line={() => "Solution architecture. C4 level 2. The apps and data stores, and how they talk."} />}
    </Answered>
  );
}

/** The name of the container a component diagram draws: its boundary. */
function boundaryName(diagram: C4Diagram): string | undefined {
  return diagram.elements.find((element) => element.kind === "boundary")?.label;
}

function ComponentLevel({ container }: { container: string }) {
  const query = useC4Component(container);
  return (
    <Answered query={query}>
      {(source) => (
        <Drawn
          source={source}
          level="component"
          container={boundaryName}
          line={(diagram) => `Solution architecture. C4 level 3. What ${boundaryName(diagram) ?? "this container"} is made of, from the code.`}
        />
      )}
    </Answered>
  );
}

/** With several systems the server refuses to choose, so the page lists them from the context diagram. */
function ChooseSystem() {
  const query = useC4Context();
  return (
    <Answered query={query}>
      {(source) => {
        const parsed = parseC4(source);
        const systems = "diagram" in parsed ? parsed.diagram.elements.filter((element) => element.kind === "system" && !element.external && element.link !== undefined) : [];
        if (systems.length <= 1) return <ContainerLevel system={systems[0]?.link} />;
        return (
          <section aria-label="Systems" className="space-y-2">
            <h1 className="text-[22px] leading-7 font-semibold">Solution architecture</h1>
            <p className="text-[13px] leading-4 text-muted-foreground">This store has several systems. Choose one to see its containers.</p>
            <ul className="space-y-1 text-sm">
              {systems.map((element) => (
                <li key={element.alias}>
                  <AppLink href={systemHref(element.link ?? element.alias)} className="text-primary underline underline-offset-2">
                    {element.label}
                  </AppLink>
                </li>
              ))}
            </ul>
          </section>
        );
      }}
    </Answered>
  );
}

/** The System space: the context diagram, a system's containers, or a container's components. */
export function C4Page({ view, system, container }: C4PageProps) {
  if (view === undefined) return <ContextLevel />;
  if (container !== undefined) return <ComponentLevel container={container} />;
  return system === undefined ? <ChooseSystem /> : <ContainerLevel system={system} />;
}

/** `<system> / <container>` for the component diagram; the system links back to its containers. */
export function ComponentTrail({ container }: { container: string }) {
  const hld = useHld();
  const parts = [...(hld.data?.systems ?? []), ...(hld.data?.containers ?? [])];
  const own = parts.find((part) => part.id === container);
  const seen = new Set([container]);
  let above = own?.parent;
  while (above !== undefined && !seen.has(above) && !(hld.data?.systems ?? []).some((part) => part.id === above)) {
    seen.add(above);
    above = parts.find((part) => part.id === above)?.parent;
  }
  const system = (hld.data?.systems ?? []).find((part) => part.id === above);
  return (
    <span aria-label="Diagram path" className="ml-auto pb-2 text-foreground">
      {system === undefined ? null : (
        <>
          <AppLink href={systemHref(system.id)} className="hover:text-primary">
            {system.title}
          </AppLink>
          {" / "}
        </>
      )}
      {own?.title ?? container.split("/").pop() ?? container}
    </span>
  );
}
