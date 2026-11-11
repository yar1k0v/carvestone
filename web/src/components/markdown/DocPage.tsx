import { useLocation } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import type { DocRow } from "@/api/types/DocRow";
import { CONNECTED_ID, ConnectedTo } from "@/components/docs/ConnectedTo";
import { parseFact, readDoc } from "@/components/docs/lead";
import { ago, chipText, groupLabel, libraryHref, rememberDoc } from "@/components/docs/library";
import { useDocRows } from "@/components/docs/useDocs";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { StaleBanner } from "@/components/item/StaleBanner";
import { TrustBadge } from "@/components/item/TrustBadge";
import { isItemView, useItemQuery } from "@/components/item/useItem";
import { Loading } from "@/components/ui/skeleton";
import { type Crumb, useCrumbs, useOutline } from "@/shell/pageFrame";
import { AppLink } from "./AppLink";
import { Markdown } from "./Markdown";
import { useGo, useItemRows, useResolver } from "./useResolver";

/** Where the doc sits: under its family in the library. */
function crumbsOf(row: DocRow | undefined, title: string): Crumb[] {
  const family = row === undefined || row.group === "other" ? [] : [{ label: groupLabel(row.group), href: `/docs?group=${encodeURIComponent(row.group)}` }];
  return [{ label: "Library", href: libraryHref() }, ...family, { label: title }];
}

function Byline({ row, status, now }: { row: DocRow | undefined; status: string | null; now: Date }) {
  const parts = [
    row?.updated === undefined ? null : `Updated ${ago(row.updated, now)}`,
    row === undefined ? null : `${row.minutes} min read`,
    status,
  ].filter((part) => part !== null);
  if (parts.length === 0) return null;
  return (
    <span title={row?.updated} className="text-muted-foreground">
      {parts.join(" · ")}
    </span>
  );
}

/** One document as a reading page: one column of text, then what it connects to. */
export function DocPage({ docId }: { docId: string }) {
  const query = useItemQuery(docId);
  const rows = useItemRows();
  const docs = useDocRows();
  const resolver = useResolver();
  const go = useGo();
  const hash = useLocation({ select: (location) => location.hash });
  const [now] = useState(() => new Date());
  const doc = query.data && isItemView(query.data) ? query.data : null;
  const row = docs.data?.find((candidate) => candidate.id === docId);

  useEffect(() => {
    if (doc !== null) rememberDoc(doc.id);
  }, [doc]);

  const title = doc !== null && typeof doc.fields.title === "string" ? doc.fields.title : (row?.title ?? docId);
  const read = useMemo(() => readDoc(doc?.body ?? "", title), [doc?.body, title]);
  // A value with no link or code says something "Connected to" cannot, so it stays under the title.
  const notes = useMemo(() => {
    const kind = row?.kind?.toLowerCase();
    return read.facts.filter((fact) => parseFact(fact.value).parts.length === 0 && !(fact.key === "Kind" && fact.value.toLowerCase() === kind));
  }, [read.facts, row?.kind]);
  const outline = useMemo(() => {
    if (doc === null) return [];
    const sections = doc.sections.filter((section) => section.level <= 3 && section.title.trim().toLowerCase() !== title.trim().toLowerCase());
    return [...sections.map((section) => ({ id: section.anchor, title: section.title, level: section.level })), { id: CONNECTED_ID, title: "Connected to" }];
  }, [doc, title]);

  useOutline(outline);
  useCrumbs(crumbsOf(row, title));

  if (query.isError) return <ErrorNotice error={query.error} />;
  if (query.data !== undefined && doc === null) return <ErrorNotice error={new Error(`${docId} is not a document`)} />;

  const status = doc !== null && typeof doc.fields.status === "string" ? doc.fields.status : (row?.status ?? null);

  return (
    <article className="min-w-0 pb-10">
      {doc === null ? null : <StaleBanner stale={doc.stale} />}
      <header className="mb-8">
        <h1 className="text-[30px] leading-9 font-[650]">{title}</h1>
        <p className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          {row === undefined ? null : <span className="rounded-md border bg-muted px-2 py-px text-xs font-medium">{chipText(row)}</span>}
          <Byline row={row} status={status} now={now} />
          {doc === null ? null : <TrustBadge trust={doc.trust} />}
          {doc === null ? null : (
            <AppLink href={`/items/${doc.id}`} className="ml-auto text-xs text-muted-foreground hover:text-foreground hover:underline">
              Details
            </AppLink>
          )}
        </p>
        {notes.length === 0 ? null : <p className="mt-1.5 text-sm text-muted-foreground">{notes.map((note) => `${note.key}: ${note.value}`).join(" · ")}</p>}
      </header>
      {doc === null ? (
        <Loading lines={10} />
      ) : (
        <>
          <Markdown source={read.body} resolver={resolver} fromId={doc.id} onNavigate={go} activeAnchor={hash} />
          <ConnectedTo doc={doc} facts={read.facts} body={read.body} rows={rows.data ?? []} resolver={resolver} />
        </>
      )}
    </article>
  );
}
