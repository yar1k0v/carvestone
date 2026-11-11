import { useMemo, useState } from "react";
import type { Feature } from "@/api/types/Feature";
import { Empty, Tag, link } from "@/components/architecture/service/parts";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { isItemView, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { Input } from "@/components/ui/input";
import { Chips, PageHeader } from "@/components/ui/page";
import { LoadingRows } from "@/components/ui/skeleton";
import { useOutline } from "@/shell/pageFrame";
import { type Audience, type Scope, audienceOf, featureHref, filterFeatures, inScope } from "./model";
import { useProduct } from "./useProduct";

const CELL = "px-3 py-[11px]";
const KEEP = "whitespace-nowrap";
const EDGE = "first:pl-4 last:pr-4";
const HEAD = `px-3 py-2.5 text-[12.5px] font-medium whitespace-nowrap ${EDGE}`;

const SCOPES: { value: Scope; label: string }[] = [
  { value: "all", label: "All" },
  { value: "work", label: "Has open work" },
  { value: "stale", label: "Docs out of date" },
];

function FeatureTable({ features }: { features: Feature[] }) {
  const [text, setText] = useState("");
  const [scope, setScope] = useState<Scope>("all");
  const shown = filterFeatures(features, text).filter((feature) => inScope(feature, scope));
  return (
    <>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <Input
          type="search"
          aria-label="Filter features"
          placeholder={`Filter ${features.length} ${features.length === 1 ? "feature" : "features"}`}
          value={text}
          onChange={(event) => setText(event.target.value)}
          className="min-w-0 flex-[0_1_280px]"
        />
        <Chips label="Show" options={SCOPES} value={scope} onChange={setScope} />
      </div>
      <div className="slab overflow-x-auto">
        <table className="w-full min-w-[780px] text-sm">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th scope="col" className={HEAD}>
                Feature
              </th>
              <th scope="col" className={HEAD}>
                State
              </th>
              <th scope="col" className={HEAD}>
                Available on
              </th>
              <th scope="col" className={HEAD}>
                Open tasks
              </th>
              <th scope="col" className={HEAD}>
                Services
              </th>
            </tr>
          </thead>
          <tbody className="[&>tr]:border-t">
            {shown.map((feature) => (
              <tr key={feature.id}>
                <td className={`${CELL} ${EDGE}`}>
                  <AppLink href={featureHref(feature.id)} title={feature.summary} className="font-[550] hover:text-primary">
                    {feature.title}
                  </AppLink>
                </td>
                <td className={`${CELL} ${EDGE} ${KEEP}`}>{feature.status === undefined ? null : <Tag>{feature.status}</Tag>}</td>
                <td className={`${CELL} ${EDGE} ${KEEP} text-muted-foreground`}>{feature.platforms.join(", ")}</td>
                <td title={`${feature.done_tasks} done. Related tasks are not linked to this feature, they only share something with it.`} className={`${CELL} ${EDGE} ${KEEP}`}>
                  <span className="font-mono text-[13px] tabular-nums">{feature.open_tasks}</span>
                  {feature.inferred_open_tasks > 0 ? <span className="text-muted-foreground"> +{feature.inferred_open_tasks} related</span> : null}
                </td>
                <td className={`${CELL} ${EDGE} text-muted-foreground`}>{feature.services.join(", ")}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {shown.length === 0 ? <p className="px-4 py-3 text-muted-foreground">No feature matches this filter.</p> : null}
      </div>
      <p className="mt-2 text-xs text-muted-foreground md:hidden">Swipe the table to see more.</p>
    </>
  );
}

function Audiences({ groups }: { groups: Audience[] }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {groups.map((group) => (
        <div key={`${group.name}:${group.text}`} className="slab p-4 text-[0.9375rem] leading-[1.375rem] text-muted-foreground">
          {group.name === "" ? null : <strong className="mb-1 block font-semibold text-foreground">{group.name}</strong>}
          {group.text}
        </div>
      ))}
    </div>
  );
}

/** The first screen: what the product is, whom it serves and its features. */
export function ProductPage() {
  const product = useProduct();
  const overview = product.data?.overview ?? null;
  const features = product.data?.features ?? [];
  const doc = useItemQuery(overview?.id ?? "", overview !== null);
  const groups = useMemo(() => (doc.data !== undefined && isItemView(doc.data) ? audienceOf(doc.data.body) : []), [doc.data]);
  useOutline([
    { id: "overview", title: "Overview" },
    { id: "features", title: "Features" },
    ...(groups.length > 0 ? [{ id: "who-it-serves", title: "Who it serves" }] : []),
  ]);

  return (
    <article className="pb-8">
      <div id="overview" className="scroll-mt-6">
        <PageHeader title={overview?.title ?? "Product"} description={overview?.summary} className="mb-3" />
      </div>
      <div className="text-sm leading-[1.4]">
        {product.error ? <ErrorNotice error={product.error} /> : null}
        {overview !== null ? (
          <AppLink href={hrefFor(overview.id)} className={link}>
            Read the full overview
          </AppLink>
        ) : product.data ? (
          <Empty>No product overview yet. A doc named product-overview, or of kind product, shows here.</Empty>
        ) : null}
        <section aria-labelledby="features" className="mt-5">
          <h2 id="features" className="sr-only">
            Features
          </h2>
          {product.isPending ? <LoadingRows /> : null}
          {product.data && features.length === 0 ? <Empty>No features yet. A doc named feature-something, or of kind feature, shows here.</Empty> : null}
          {features.length > 0 ? <FeatureTable features={features} /> : null}
        </section>
        {groups.length > 0 ? (
          <section aria-labelledby="who-it-serves" className="mt-9">
            <h2 id="who-it-serves" className="mb-3 scroll-mt-6 text-[1.375rem] leading-7 font-semibold">
              Who it serves
            </h2>
            <Audiences groups={groups} />
          </section>
        ) : null}
      </div>
    </article>
  );
}
