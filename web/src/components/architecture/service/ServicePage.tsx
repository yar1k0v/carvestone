import type { ArchGraph } from "@/api/types/ArchGraph";
import type { Hld } from "@/api/types/Hld";
import type { Part } from "@/api/types/Part";
import { serviceHref } from "@/components/graph/serviceHref";
import { useHld } from "@/components/graph/useDesign";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { useRowsOfType } from "@/components/planning/useRows";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { type Crumb, useCrumbs, useOutline } from "@/shell/pageFrame";
import { Commits } from "./Commits";
import { Connections } from "./Connections";
import { Domain } from "./Domain";
import { Inside } from "./Inside";
import { ServiceList } from "./ServiceList";
import { WrittenAndCode } from "./Written";
import { type ServiceTab, connectionRows, disagreements, findPart, keyFacts, plainNote, runningOn, titlesOf, writtenLinks } from "./model";
import { Answer, Block, Empty, Flag, link } from "./parts";
import { useArchGraph, useExplain } from "./useService";

const TABS: { tab: ServiceTab | undefined; label: string }[] = [
  { tab: undefined, label: "Inside" },
  { tab: "domain", label: "Domain" },
  { tab: "flows", label: "Flows" },
  { tab: "commits", label: "Commits" },
];

interface Tabbed {
  tab: ServiceTab | undefined;
  onTab: (tab: ServiceTab | undefined) => void;
}

/** System architecture, the elements above the service by their `parent`, then the service: the address may not show them. */
function crumbsOf(part: Part, hld: Hld | undefined, graph: ArchGraph | undefined, titleOf: (id: string) => string): Crumb[] {
  const above: Crumb[] = [];
  const seen = new Set([part.id]);
  for (let at = part.parent; at !== undefined && !seen.has(at); at = findPart(at, hld, graph)?.parent) {
    seen.add(at);
    above.unshift({ label: titleOf(at), href: serviceHref(at) });
  }
  return [{ label: "System architecture", href: "/architecture" }, ...above, { label: part.title }];
}

function Service({ part, hld, graph, tab, onTab }: { part: Part; hld: Hld | undefined; graph: ArchGraph | undefined } & Tabbed) {
  useOutline([]);
  const titleOf = titlesOf(hld, graph);
  useCrumbs(crumbsOf(part, hld, graph, titleOf));
  const explain = useExplain(part.id);
  const tasks = useRowsOfType("task");
  const links = writtenLinks(hld, graph);
  const facts = explain.data;
  const calledBy = connectionRows("called-by", part.id, links, facts?.inbound, titleOf);
  const calls = connectionRows("calls", part.id, links, facts?.outbound, titleOf);
  const differences = disagreements(part.id, graph, titleOf);
  const running = runningOn(part, tasks.data ?? []).length;
  const current = TABS.find((entry) => entry.tab === tab) ?? TABS[0];
  return (
    <article className="flex flex-col gap-4 pb-8">
      <div className="flex flex-wrap items-end gap-x-6 gap-y-2.5">
        <div className="min-w-0 flex-[1_1_420px]">
          <h1 className="text-[26px] leading-8 font-[650] tracking-[-0.02em]">{part.title}</h1>
          <p className="mt-0.5 text-[15px] text-foreground/85">{part.description ?? "No description written yet."}</p>
          <AppLink href={hrefFor(part.id)} className={`${link} text-xs`}>
            Open the written page
          </AppLink>
        </div>
        {differences.length === 0 ? null : (
          <AppLink href={`${serviceHref(part.id)}?tab=flows`}>
            <Flag>
              {differences.length} {differences.length === 1 ? "difference" : "differences"} with the docs
            </Flag>
          </AppLink>
        )}
      </div>
      {facts?.note === undefined ? null : <p className="text-xs text-muted-foreground">{plainNote(facts.note)}</p>}
      <div className="flex flex-col gap-4 text-sm leading-[1.4]">
        <div role="group" aria-label="Key facts" className="grid gap-2.5 [grid-template-columns:repeat(auto-fit,minmax(150px,1fr))]">
          {keyFacts(part, calledBy, calls, running).map((fact) => (
            <div key={fact.label} className="slab px-3.5 py-2.5">
              <p className="text-xs text-muted-foreground">{fact.label}</p>
              <p className="font-semibold">{fact.value}</p>
            </div>
          ))}
        </div>
        <div role="tablist" aria-label="Service sections" className="flex flex-wrap gap-5 border-b">
          {TABS.map((entry) => {
            const selected = entry === current;
            return (
              <button
                key={entry.label}
                type="button"
                role="tab"
                id={`service-tab-${entry.tab ?? "inside"}`}
                aria-selected={selected}
                aria-controls="service-panel"
                onClick={() => onTab(entry.tab)}
                className={cn("py-[7px]", selected ? "border-b-2 border-primary font-semibold text-primary" : "text-muted-foreground hover:text-foreground")}
              >
                {entry.label}
              </button>
            );
          })}
        </div>
        <div
          role="tabpanel"
          id="service-panel"
          className="[&>section:first-child]:border-t-0 [&>section:first-child]:pt-0"
          aria-labelledby={`service-tab-${current?.tab ?? "inside"}`}>
          {current?.tab === undefined ? (
            <>
              <Block id="inside" title="Inside" lead="Modules found in the code. Choose one to see what it calls and what calls it.">
                <Answer query={explain} label="Reading the code..." quiet>
                  {(data) => <Inside explain={data} title={part.title} />}
                </Answer>
              </Block>
              <Block id="connections" title="Connections">
                <Connections calledBy={calledBy} calls={calls} />
              </Block>
            </>
          ) : null}
          {current?.tab === "domain" ? (
            <Block id="domain-model" title="Domain model">
              <Domain part={part.id} />
            </Block>
          ) : null}
          {current?.tab === "flows" ? (
            <Block id="flows" title="Flows and design" lead="What the docs intend, next to what the code does now.">
              <WrittenAndCode id={part.id} explain={facts} differences={differences} />
            </Block>
          ) : null}
          {current?.tab === "commits" ? (
            <Block id="commits" title="Recent commits">
              <Commits id={part.id} />
            </Block>
          ) : null}
        </div>
      </div>
    </article>
  );
}

/** A system is the top step of the ladder: what it is and the services in it. */
function System({ part, hld, graph }: { part: Part; hld: Hld | undefined; graph: ArchGraph | undefined }) {
  const services = (hld?.containers ?? []).filter((entry) => entry.parent === part.id);
  return (
    <article className="pb-8">
      <PageHeader title={part.title} description={part.description ?? "No description written yet."} />
      {services.length === 0 ? <Empty>No services listed in this system.</Empty> : <ServiceList services={services} hld={hld} graph={graph} />}
    </article>
  );
}

/** `/architecture/<element id>`: one service, with its key facts and a tab for each section. */
export function ServicePage({ target, tab, onTab }: { target: string } & Tabbed) {
  const hld = useHld();
  const graph = useArchGraph();
  const part = findPart(target, hld.data, graph.data);
  if (part === undefined) {
    if (hld.isPending || graph.isPending) return <Loading lines={6} label="Reading the design..." className="max-w-3xl" />;
    const failed = hld.error ?? graph.error;
    if (failed !== null) return <ErrorNotice error={failed} />;
    return <Empty>No service called {target}.</Empty>;
  }
  if (part.type === "system") return <System part={part} hld={hld.data} graph={graph.data} />;
  return <Service part={part} hld={hld.data} graph={graph.data} tab={tab} onTab={onTab} />;
}
