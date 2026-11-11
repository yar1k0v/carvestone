import type { ArchGraph } from "@/api/types/ArchGraph";
import type { Hld } from "@/api/types/Hld";
import type { Part } from "@/api/types/Part";
import { serviceHref } from "@/components/graph/serviceHref";
import { AppLink } from "@/components/markdown/AppLink";
import { codeConnections, driftRows, writtenLinks } from "./model";
import { Flag, link } from "./parts";

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** The services as a plain list: what each is and how many it connects to. */
export function ServiceList({ services, hld, graph }: { services: Part[]; hld: Hld | undefined; graph: ArchGraph | undefined }) {
  const links = codeConnections(services, graph, writtenLinks(hld, graph));
  return (
    <ul aria-label="Services" className="slab divide-y">
      {services.map((part) => {
        const calls = links.filter((entry) => entry.from === part.id).length;
        const calledBy = links.filter((entry) => entry.to === part.id).length;
        const drift = driftRows(part.id, graph).length;
        return (
          <li key={part.id} className="flex flex-wrap items-baseline gap-x-4 gap-y-0.5 px-4 py-2.5">
            <div className="min-w-0 flex-1 basis-64">
              <AppLink href={serviceHref(part.id)} className={`font-medium ${link}`}>
                {part.title}
              </AppLink>
              {part.technology === undefined ? null : <span className="ml-2 text-xs text-muted-foreground">{part.technology}</span>}
              {part.description === undefined ? null : <p className="text-sm text-muted-foreground">{part.description}</p>}
            </div>
            <span className="flex flex-wrap items-baseline gap-2 text-xs whitespace-nowrap text-muted-foreground">
              {count(calls, "service called", "services called")}, called by {calledBy}
              {drift === 0 ? null : <Flag>{count(drift, "difference", "differences")}</Flag>}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
