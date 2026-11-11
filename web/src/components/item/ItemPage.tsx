import { useQueryClient } from "@tanstack/react-query";
import { useLocation, useRouter } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { keys } from "@/api/keys";
import type { CodeView } from "@/api/types/CodeView";
import type { ItemView } from "@/api/types/ItemView";
import { dropTitle } from "@/components/docs/lead";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { codeHref } from "@/components/markdown/links";
import { useGo, useResolver } from "@/components/markdown/useResolver";
import { Loading } from "@/components/ui/skeleton";
import { StatusBadge } from "@/components/ui/status";
import { useOutline } from "@/shell/pageFrame";
import { Backlinks } from "./Backlinks";
import { ErrorNotice } from "./ErrorNotice";
import { ArchExtras, ChangedFiles, Libraries } from "./Extras";
import { FieldTable } from "./FieldTable";
import { HistoryTab } from "./HistoryTab";
import { ProvenanceTab } from "./ProvenanceTab";
import { StaleBanner } from "./StaleBanner";
import { TrustBadge } from "./TrustBadge";
import { isCodeView, isItemView, useCachedRow, useItemQuery } from "./useItem";

const ARCH_TYPES = new Set(["system", "container", "component"]);

type TabId = "content" | "history" | "provenance" | "changed";

function Tabs({ tabs, active, onSelect }: { tabs: { id: TabId; label: string }[]; active: TabId; onSelect: (id: TabId) => void }) {
  return (
    <div role="tablist" className="mb-4 flex gap-1 border-b">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          role="tab"
          type="button"
          aria-selected={tab.id === active}
          onClick={() => onSelect(tab.id)}
          className={`-mb-px border-b-2 px-3 py-1.5 text-sm ${
            tab.id === active ? "border-primary font-medium" : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

function Badge({ children }: { children: string }) {
  return <span className="rounded-full border px-2 py-0.5 text-xs text-muted-foreground">{children}</span>;
}

function ItemBody({ item }: { item: ItemView }) {
  const resolver = useResolver();
  const go = useGo();
  const hash = useLocation({ select: (location) => location.hash });
  const [tab, setTab] = useState<TabId>("content");
  const outline = useMemo(
    () => (tab === "content" ? item.sections.map((section) => ({ id: section.anchor, title: section.title, level: section.level })) : []),
    [tab, item.sections],
  );
  useOutline(outline);

  // The server leaves `changed_files` out when a task has none.
  const changed = item.changed_files ?? [];
  const tabs: { id: TabId; label: string }[] = [
    { id: "content", label: "Content" },
    { id: "history", label: "History" },
    { id: "provenance", label: "Provenance" },
    ...(item.type === "task" ? [{ id: "changed" as const, label: `Changed files (${changed.length})` }] : []),
  ];
  const title = typeof item.fields.title === "string" ? item.fields.title : item.id;
  const status = typeof item.fields.status === "string" ? item.fields.status : null;

  return (
    <article>
      <StaleBanner stale={item.stale} />
      <header className="mb-4">
        <p className="eyebrow">{item.short ?? item.id}</p>
        <h1 className="text-2xl font-semibold">{title}</h1>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <Badge>{item.type}</Badge>
          <StatusBadge status={status} />
          <TrustBadge trust={item.trust} />
          {item.type === "doc" ? (
            <AppLink href={`/docs/${item.id.slice("docs/".length)}`} className="text-xs text-muted-foreground underline underline-offset-2">
              Open in docs
            </AppLink>
          ) : null}
        </div>
      </header>
      <Tabs tabs={tabs} active={tab} onSelect={setTab} />
      <div role="tabpanel">
        {tab === "content" ? (
          <div className="space-y-6">
            <FieldTable fields={item.fields} resolver={resolver} />
            <Libraries libraries={item.libraries ?? []} />
            {ARCH_TYPES.has(item.type) ? <ArchExtras item={item} /> : null}
            <Markdown source={dropTitle(item.body, title)} resolver={resolver} fromId={item.id} onNavigate={go} activeAnchor={hash} />
            <Backlinks items={item.backlinks} />
          </div>
        ) : null}
        {tab === "history" ? <HistoryTab id={item.id} sections={item.sections} /> : null}
        {tab === "provenance" ? <ProvenanceTab item={item} /> : null}
        {tab === "changed" ? <ChangedFiles files={changed} /> : null}
      </div>
    </article>
  );
}

/** A symbol or file that no item owns: where it is, and which items link to it. */
function CodeBody({ code }: { code: CodeView }) {
  return (
    <article className="space-y-4">
      <header>
        <p className="font-mono text-sm text-muted-foreground">{code.type}</p>
        <h1 className="font-mono text-xl font-semibold">{code.id}</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          {code.path}, lines {code.lines[0]} to {code.lines[1]}
        </p>
        <AppLink href={codeHref(code.id)} className="text-sm text-primary underline underline-offset-2">
          Open in the code explorer
        </AppLink>
      </header>
      <Backlinks items={code.backlinks} />
    </article>
  );
}

/** Any item by the id the route spells; a short id moves to the path id. */
export function ItemPage({ id }: { id: string }) {
  const query = useItemQuery(id);
  const cached = useCachedRow(id);
  const router = useRouter();
  const queryClient = useQueryClient();
  const hash = useLocation({ select: (location) => location.hash });
  const data = query.data;
  const canonical = data !== undefined && isItemView(data) && data.id !== id ? data : null;

  useEffect(() => {
    if (canonical === null) return;
    // The reply is already the canonical item, so the new route needs no second request.
    queryClient.setQueryData(keys.items.get(canonical.id), canonical);
    router.history.replace(`/items/${canonical.id}${hash ? `#${hash}` : ""}`);
  }, [canonical, hash, queryClient, router]);

  if (query.isPending) {
    // Any list already fetched names the item, so only the body waits.
    return (
      <article>
        <header className="mb-6">
          <p className="eyebrow">{cached?.short ?? id}</p>
          <h1 className="text-2xl font-semibold">{cached?.title ?? id}</h1>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            {cached ? <Badge>{cached.type}</Badge> : null}
            <StatusBadge status={cached?.status ?? null} />
          </div>
        </header>
        <Loading lines={8} className="max-w-3xl" />
      </article>
    );
  }
  if (query.isError) return <ErrorNotice error={query.error} />;
  if (canonical !== null) return null;
  if (isItemView(query.data)) return <ItemBody key={query.data.id} item={query.data} />;
  if (isCodeView(query.data)) return <CodeBody code={query.data} />;
  return <ErrorNotice error={new Error(`${id} is a section, not an item`)} />;
}
