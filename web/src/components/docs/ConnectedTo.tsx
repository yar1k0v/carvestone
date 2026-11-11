import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useMemo } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { History } from "@/api/types/History";
import type { HistoryEntry } from "@/api/types/HistoryEntry";
import type { ItemView } from "@/api/types/ItemView";
import type { ListRow } from "@/api/types/ListRow";
import { AppLink } from "@/components/markdown/AppLink";
import type { Resolver } from "@/components/markdown/links";
import { More } from "@/components/ui/page";
import { StatusBadge } from "@/components/ui/status";
import { type Connection, connectionsOf } from "./connections";
import type { Fact } from "./lead";

/** The element id of the heading, for "On this page". */
export const CONNECTED_ID = "connected-to";

const SHOWN = 6;
const COMMITS = 20;

function textList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string") : [];
}

function useCommits(id: string): HistoryEntry[] {
  const params = { limit: COMMITS };
  const query = useQuery({
    queryKey: keys.history(id, params),
    queryFn: () => api.get<History>(`/api/history/${id.split("/").map(encodeURIComponent).join("/")}`, params),
  });
  return query.data?.entries ?? [];
}

function Group<T>({ label, rows, children }: { label: string; rows: T[]; children: (row: T) => ReactNode }) {
  if (rows.length === 0) return null;
  return (
    <section aria-label={label}>
      <h3 className="mb-2 text-sm font-medium">
        {label} <span className="font-normal text-muted-foreground tabular-nums">{rows.length}</span>
      </h3>
      <More rows={rows} limit={SHOWN}>
        {(shown) => <ul className="slab divide-y overflow-hidden">{shown.map(children)}</ul>}
      </More>
    </section>
  );
}

function Entry({ entry }: { entry: Connection }) {
  return (
    <li className="flex items-baseline gap-3 px-3.5 py-2.5 text-sm">
      <div className="min-w-0 flex-1">
        {entry.short === undefined ? null : <span className="mr-2 font-mono text-xs text-muted-foreground">{entry.short}</span>}
        <AppLink href={entry.href} title={entry.hint} className="font-medium hover:text-primary">
          {entry.title}
        </AppLink>
        <p className="text-muted-foreground">{entry.relation}</p>
      </div>
      {entry.status === undefined ? null : <StatusBadge status={entry.status} />}
    </li>
  );
}

function Commit({ commit }: { commit: HistoryEntry }) {
  return (
    <li className="px-3.5 py-2.5 text-sm">
      <p className="min-w-0">
        <span title={commit.sha} className="mr-2 font-mono text-xs text-muted-foreground">
          {commit.sha.slice(0, 7)}
        </span>
        <span className="font-medium">{commit.subject}</span>
      </p>
      <p className="text-muted-foreground">
        Changed this document on {commit.date.slice(0, 10)}, by {commit.author}
      </p>
    </li>
  );
}

/** What a doc connects to, grouped, each entry saying how. It closes the reading page. */
export function ConnectedTo({ doc, facts, body, rows, resolver }: { doc: ItemView; facts: Fact[]; body: string; rows: ListRow[]; resolver: Resolver }) {
  const commits = useCommits(doc.id);
  const { documents, tasks, architecture } = useMemo(
    () =>
      connectionsOf({
        self: doc.id,
        facts,
        links: [...textList(doc.fields.links), ...textList(doc.fields.components)],
        body,
        backlinks: doc.backlinks,
        rows,
        resolver,
      }),
    [doc, facts, body, rows, resolver],
  );
  const empty = documents.length + tasks.length + architecture.length + commits.length === 0;
  const entry = (row: Connection) => <Entry key={row.key} entry={row} />;

  return (
    <section aria-labelledby={CONNECTED_ID} className="mt-12 border-t pt-8">
      <h2 id={CONNECTED_ID} className="mb-5 scroll-mt-6 text-[1.375rem] leading-[1.875rem] font-semibold tracking-[-0.01em]">
        Connected to
      </h2>
      {empty ? (
        <p className="text-muted-foreground">Nothing names this document yet, and it names nothing else.</p>
      ) : (
        <div className="space-y-6">
          <Group label="Documents" rows={documents}>
            {entry}
          </Group>
          <Group label="Tasks" rows={tasks}>
            {entry}
          </Group>
          <Group label="Architecture" rows={architecture}>
            {entry}
          </Group>
          <Group label="Commits" rows={commits}>
            {(commit) => <Commit key={commit.sha} commit={commit} />}
          </Group>
        </div>
      )}
    </section>
  );
}
