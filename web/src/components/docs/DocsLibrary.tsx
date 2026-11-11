import { Search } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import type { DocRow } from "@/api/types/DocRow";
import type { Hit } from "@/api/types/Hit";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { isItemView, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { highlight } from "@/components/search/hits";
import { Input } from "@/components/ui/input";
import { Chips, More, PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { readDoc } from "./lead";
import { ago, byDate, byName, chipText, filterDocs, groupCounts, readable, recentDocs, rememberLibrary } from "./library";
import { matchingLine } from "./matchLine";
import { Shelf, type ShelfName } from "./Shelf";
import { type DocPart, useDocParts, useDocRows, useDocTextSearch } from "./useDocs";

export interface LibrarySearch {
  q?: string;
  group?: string;
  shelf?: ShelfName;
  part?: string;
  sort?: "name";
  templates?: boolean;
}

/** Documents shown at first for a search: each reads its text to find the matching line. */
const FIRST_FOUND = 40;

function hrefOf(search: LibrarySearch): string {
  const params = new URLSearchParams();
  if (search.q) params.set("q", search.q);
  if (search.group) params.set("group", search.group);
  if (search.part) params.set("part", search.part);
  if (search.sort) params.set("sort", search.sort);
  if (search.templates) params.set("templates", "true");
  const text = params.toString();
  return text ? `/docs?${text}` : "/docs";
}

/** What a found doc says, with the words of the search marked. */
function Said({ text, query }: { text: string; query: string }) {
  return (
    <p className="line-clamp-2 text-sm text-muted-foreground">
      {highlight(text, query).map((piece, at) =>
        piece.match ? (
          <mark key={at} className="rounded-sm bg-primary/10 text-foreground">
            {piece.text}
          </mark>
        ) : (
          <span key={at}>{piece.text}</span>
        ),
      )}
    </p>
  );
}

function Row({ row, now, href, children }: { row: DocRow; now: Date; href?: string; children?: ReactNode }) {
  return (
    <li className="relative grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-6 px-4 py-2.5 transition-colors hover:bg-accent/50">
      <div className="min-w-0">
        <AppLink href={href ?? hrefFor(row.id)} className="font-medium after:absolute after:inset-0">
          {row.title}
        </AppLink>
        {children ?? (row.summary === undefined ? null : <p className="truncate text-sm text-muted-foreground">{row.summary}</p>)}
      </div>
      <div className="flex shrink-0 items-baseline gap-3 text-xs text-muted-foreground">
        <span className="hidden whitespace-nowrap md:inline">{chipText(row)}</span>
        {row.updated === undefined ? null : (
          <span title={row.updated} className="whitespace-nowrap">
            {ago(row.updated, now)}
          </span>
        )}
        <span className="w-12 text-right whitespace-nowrap tabular-nums">{row.minutes} min</span>
      </div>
    </li>
  );
}

/** A doc found by its text. The answer names the section, so the doc is read for the line itself. */
function FoundRow({ row, hit, query, now }: { row: DocRow; hit: Hit; query: string; now: Date }) {
  const item = useItemQuery(row.id);
  const body = item.data !== undefined && isItemView(item.data) ? item.data.body : null;
  const line = useMemo(() => (body === null ? null : matchingLine(readDoc(body, row.title).body, query)), [body, row.title, query]);
  return (
    <Row row={row} now={now} href={hrefFor(row.id, line?.anchor ?? hit.anchor)}>
      {line === null ? undefined : <Said text={line.text} query={query} />}
    </Row>
  );
}

function Rows({ rows, now, hits, query }: { rows: DocRow[]; now: Date; hits: Map<string, Hit>; query: string }) {
  return (
    <ul className="slab divide-y overflow-hidden">
      {rows.map((row) => {
        const hit = hits.get(row.id);
        return hit === undefined ? <Row key={row.id} row={row} now={now} /> : <FoundRow key={row.id} row={row} hit={hit} query={query} now={now} />;
      })}
    </ul>
  );
}

function Continue({ rows }: { rows: DocRow[] }) {
  const [ids] = useState(recentDocs);
  const opened = ids.flatMap((id) => rows.filter((row) => row.id === id)).slice(0, 4);
  if (opened.length === 0) return null;
  return (
    <section aria-label="Continue reading" className="space-y-1.5">
      <h2 className="eyebrow">Continue reading</h2>
      <ul className="flex flex-wrap gap-x-5 gap-y-1 text-sm">
        {opened.map((row) => (
          <li key={row.id} className="min-w-0">
            <AppLink href={hrefFor(row.id)} className="text-primary hover:underline">
              {row.title}
            </AppLink>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The parts that hold at least one of `rows`, largest first. */
function partChips(parts: DocPart[], rows: DocRow[]): { value: string; label: string; count: number }[] {
  return parts
    .map((part) => ({ value: part.id, label: part.title, count: rows.filter((row) => part.docs.has(row.id)).length }))
    .filter((part) => part.count > 0)
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}

/**
 * The docs as a library: the newest first, under date headings, so a recent doc is on screen with
 * no typing or scrolling. The box searches titles and the text inside the docs, and the chips
 * narrow the same list. All of it sits in the address, so the way back from a doc lands where the
 * reader was.
 */
export function DocsLibrary({ search, onSearch }: { search: LibrarySearch; onSearch: (next: LibrarySearch) => void }) {
  if (search.shelf !== undefined) return <Shelf name={search.shelf} />;
  return <Documents search={search} onSearch={onSearch} />;
}

function Documents({ search, onSearch }: { search: LibrarySearch; onSearch: (next: LibrarySearch) => void }) {
  const docs = useDocRows();
  const parts = useDocParts();
  const found = useDocTextSearch(search.q ?? "");
  const [now] = useState(() => new Date());
  useEffect(() => rememberLibrary(hrefOf(search)), [search]);

  const all = useMemo(() => docs.data ?? [], [docs.data]);
  const visible = useMemo(() => readable(all, search.templates), [all, search.templates]);
  const chips = useMemo(() => groupCounts(visible), [visible]);
  const partOptions = useMemo(() => partChips(parts, visible), [parts, visible]);
  const inPart = parts.find((candidate) => candidate.id === search.part)?.docs;
  const rows = useMemo(() => filterDocs(all, search, found.hits, inPart), [all, search, found.hits, inPart]);
  const templates = all.filter((row) => row.template).length;
  const narrowed = Boolean(search.q || search.group || search.part);

  const set = (change: LibrarySearch) => onSearch({ ...search, ...change });

  return (
    <div className="space-y-5 pb-8">
      <PageHeader
        title="All documents"
        description={docs.data ? (narrowed ? `${rows.length} of ${visible.length} documents, newest first.` : `${visible.length} documents, newest first.`) : "Every document in this project."}
        className="mb-0"
      />

      <div className="space-y-3">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="search"
            aria-label="Search documents"
            placeholder="Search titles and the text inside documents"
            value={search.q ?? ""}
            onChange={(event) => set({ q: event.target.value || undefined })}
            className="h-10 pl-9"
          />
        </div>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <span className="w-12 text-xs text-muted-foreground">Type</span>
          <Chips
            label="Type"
            value={search.group ?? ""}
            options={[{ value: "", label: "All", count: visible.length }, ...chips]}
            onChange={(group) => set({ group: group || undefined })}
          />
        </div>
        {partOptions.length === 0 ? null : (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span className="w-12 text-xs text-muted-foreground">Part</span>
            <Chips
              label="Part"
              value={search.part ?? ""}
              options={[{ value: "", label: "All" }, ...partOptions]}
              onChange={(next) => set({ part: next || undefined })}
            />
          </div>
        )}
        <div className="flex items-center justify-end gap-3 text-xs text-muted-foreground">
          {templates === 0 ? null : (
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={search.templates === true}
                onChange={(event) => set({ templates: event.target.checked || undefined })}
              />
              Show templates
            </label>
          )}
          <button
            type="button"
            onClick={() => set({ sort: search.sort === "name" ? undefined : "name" })}
            className="rounded hover:text-foreground"
          >
            {search.sort === "name" ? "Sort by date" : "Sort by name"}
          </button>
        </div>
      </div>

      {docs.isPending ? <Loading lines={8} label="Reading the documents..." /> : null}
      {docs.isError ? <ErrorNotice error={docs.error} /> : null}
      {docs.data && !narrowed ? <Continue rows={all} /> : null}
      {search.q && found.failed ? <p className="text-sm text-muted-foreground">The text search did not answer, so only titles and summaries are searched.</p> : null}
      {search.q && found.cut ? <p className="text-sm text-muted-foreground">Too many sections match to show them all. Add a word to narrow the search.</p> : null}
      {docs.data && rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {all.length === 0 ? "No documents yet." : search.q && found.busy ? "Searching inside the documents..." : "No document matches."}{" "}
          {narrowed && !found.busy ? (
            <button type="button" onClick={() => set({ q: undefined, group: undefined, part: undefined })} className="text-primary hover:underline">
              Clear the filter
            </button>
          ) : null}
        </p>
      ) : null}
      {rows.length === 0 ? null : (
        <More rows={rows} limit={search.q ? FIRST_FOUND : rows.length}>
          {(shown) =>
            search.sort === "name" ? (
              <Rows rows={byName(shown)} now={now} hits={found.hits} query={found.query} />
            ) : (
              byDate(shown, now).map((group) => (
                <section key={group.label} aria-label={group.label} className="space-y-1.5">
                  <h2 className="eyebrow">
                    {group.label} <span className="tabular-nums opacity-70">{group.rows.length}</span>
                  </h2>
                  <Rows rows={group.rows} now={now} hits={found.hits} query={found.query} />
                </section>
              ))
            )
          }
        </More>
      )}
    </div>
  );
}
