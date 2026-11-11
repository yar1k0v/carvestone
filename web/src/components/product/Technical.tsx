import type { ReactNode } from "react";
import type { DesignDoc } from "@/api/types/DesignDoc";
import type { FeatureDetail } from "@/api/types/FeatureDetail";
import type { FeaturePart } from "@/api/types/FeaturePart";
import { Block, Empty, Tag, link } from "@/components/architecture/service/parts";
import { serviceHref } from "@/components/graph/serviceHref";
import { useContainerDocs } from "@/components/graph/useDesign";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { More } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { TIE_HINT, TIE_LABEL, type Tied, flowsOfFeature } from "./model";

const SHOWN = 12;

/** What the code knows of one service. */
interface Coverage {
  part: FeaturePart;
  count: number;
}

function Part({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="mt-6 first:mt-0">
      <h3 id={id} className="mb-2 scroll-mt-6 text-base font-semibold">
        {title}
      </h3>
      {children}
    </section>
  );
}

function TieTag<T>({ row }: { row: Tied<T> }) {
  return <Tag title={TIE_HINT[row.tie]}>{TIE_LABEL[row.tie]}</Tag>;
}

/** Says what is missing and where the whole list lives. */
function NoneTied({ what, how, section, coverage }: { what: string; how: string; section: string; coverage: Coverage[] }) {
  return (
    <Empty>
      No {what} tied to this feature yet. {how} {coverage.length === 0 ? null : "All of them are on the service pages: "}
      {coverage.map(({ part, count }, at) => (
        <span key={part.id}>
          {at === 0 ? "" : ", "}
          <AppLink href={`${serviceHref(part.id)}?tab=${section}`} className={link}>
            {part.title}
            {` (${count})`}
          </AppLink>
        </span>
      ))}
      {coverage.length === 0 ? null : "."}
    </Empty>
  );
}

function Services({ parts }: { parts: FeaturePart[] }) {
  return (
    <ul aria-label="Services" className="slab divide-y overflow-hidden">
      {parts.map((part) => (
        <li key={part.id} className="flex items-baseline gap-3 px-3.5 py-2 text-sm">
          <AppLink href={serviceHref(part.id)} className="min-w-0 flex-1 font-medium hover:text-primary">
            {part.title}
          </AppLink>
          {part.inferred ? <Tag title="Not linked. It owns code the feature doc or its tasks name.">found from code</Tag> : <Tag>linked</Tag>}
        </li>
      ))}
    </ul>
  );
}

function Flows({ parts: all, title, linked }: { parts: FeaturePart[]; title: string; linked: Set<string> }) {
  // Only a service has flow docs of its own.
  const parts = all.filter((part) => part.kind === "container");
  const answers = useContainerDocs(parts);
  if (parts.some((part) => answers.get(part.id)?.isPending === true)) return <Loading lines={2} label="Reading the flows..." className="max-w-xl" />;
  const docsOf = (part: FeaturePart) => (answers.get(part.id)?.data?.docs ?? []).filter((doc) => doc.kind === "flow");
  const seen = new Map<string, DesignDoc>(parts.flatMap(docsOf).map((doc) => [doc.id, doc]));
  const rows = flowsOfFeature([...seen.values()], title, linked);
  if (rows.length === 0) {
    const coverage = parts.map((part) => ({ part, count: docsOf(part).length }));
    return <NoneTied what="flows are" how="Link a flow doc from the feature doc and it shows here." section="flows" coverage={coverage} />;
  }
  return (
    <More rows={rows} limit={SHOWN}>
      {(shown) => (
        <ul aria-label="Flows" className="slab divide-y overflow-hidden">
          {shown.map((row) => (
            <li key={row.value.id} className="flex items-baseline gap-3 px-3.5 py-2 text-sm">
              <AppLink href={hrefFor(row.value.id)} className="min-w-0 flex-1 font-medium hover:text-primary">
                {row.value.title}
              </AppLink>
              <TieTag row={row} />
            </li>
          ))}
        </ul>
      )}
    </More>
  );
}

/** From a feature down to the code: the services it runs on, then its flows. */
export function Technical({ detail }: { detail: FeatureDetail }) {
  const parts = detail.links.parts;
  const linked = new Set(detail.links.docs.map((row) => row.id));
  return (
    <Block id="technical" title="Technical">
      {parts.length === 0 ? (
        <Empty>
          No services are tied to this feature yet, so its flows cannot be shown. Link a service from the feature doc, or name its code paths there.
        </Empty>
      ) : (
        <>
          <Part id="technical-services" title="Services">
            <Services parts={parts} />
          </Part>
          <Part id="technical-flows" title="Flows">
            <Flows parts={parts} title={detail.title} linked={linked} />
          </Part>
        </>
      )}
    </Block>
  );
}
