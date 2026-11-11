import type { ReactNode } from "react";
import type { DesignDoc } from "@/api/types/DesignDoc";
import type { Explain } from "@/api/types/Explain";
import { lldParts, lldTitle } from "@/components/graph/lldModel";
import { AppLink } from "@/components/markdown/AppLink";
import { classHref } from "@/components/architecture/symbol/href";
import { hrefFor } from "@/components/markdown/links";
import { More } from "@/components/ui/page";
import type { Disagreement } from "./model";
import { Empty, Flag, link } from "./parts";
import { useWritten } from "./useService";

/** `path#Symbol` opens the symbol, a bare path the file. */
function sourceHref(target: string): string {
  const [path, symbol] = target.split("#");
  return classHref(symbol ?? path ?? target);
}

/** `path#Symbol` names the symbol, a path its file; the last word is enough to read. */
function shortName(target: string): string {
  const [path = target, symbol] = target.split("#");
  return symbol === undefined ? (path.split("/").pop() ?? path) : symbol.split(/[.:]/).slice(-2).join(".");
}

function Doc({ doc }: { doc: DesignDoc }) {
  return (
    <li className="py-1.5">
      <AppLink href={hrefFor(doc.id)} className={`font-medium ${link}`}>
        {lldTitle(doc.title)}
      </AppLink>
      {doc.summary === undefined ? null : <p className="line-clamp-2 text-sm text-muted-foreground">{doc.summary}</p>}
      {doc.start === undefined ? null : (
        <p className="text-xs text-muted-foreground">
          Starts at{" "}
          <AppLink href={sourceHref(doc.start)} title={doc.start} className={`font-mono ${link}`}>
            {shortName(doc.start)}
          </AppLink>
        </p>
      )}
    </li>
  );
}

function DocGroup({ title, docs }: { title: string; docs: DesignDoc[] }) {
  if (docs.length === 0) return null;
  return (
    <div className="mb-3">
      <h4 className="text-sm font-semibold text-muted-foreground">
        {title} <span className="font-normal tabular-nums">{docs.length}</span>
      </h4>
      <More rows={docs} limit={6}>
        {(shown) => (
          <ul>
            {shown.map((doc) => (
              <Doc key={doc.id} doc={doc} />
            ))}
          </ul>
        )}
      </More>
    </div>
  );
}

/** What the docs intend. A part with no written design answers with an error, which only means none. */
function Intent({ id }: { id: string }) {
  const query = useWritten(id);
  if (query.isPending) return <Empty>Reading the docs...</Empty>;
  const docs = query.data?.docs ?? [];
  if (docs.length === 0) return <Empty>Nothing written yet. A doc that links this service and sits in the LLD family shows here.</Empty>;
  const { flows, components, reference } = lldParts(docs);
  return (
    <>
      <DocGroup title="Flows" docs={flows} />
      <DocGroup title="Components" docs={components} />
      <DocGroup title="Reference" docs={reference} />
    </>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="mb-3">
      <h4 className="text-sm font-semibold text-muted-foreground">{label}</h4>
      {children}
    </div>
  );
}

/** What the code shows at this moment. */
function CodeFacts({ explain }: { explain: Explain | undefined }) {
  if (explain === undefined) return <Empty>No code to read for this service.</Empty>;
  return (
    <>
      <Fact label="Starts at">
        {explain.entries.length === 0 ? (
          <Empty>No entry points found.</Empty>
        ) : (
          <More rows={explain.entries} limit={5}>
            {(shown) => (
              <ul className="text-sm">
                {shown.map((entry) => (
                  <li key={entry.target} className="flex flex-wrap items-baseline gap-x-2 py-0.5">
                    <AppLink href={sourceHref(entry.target)} title={entry.target} className={`font-mono text-[0.8125rem] ${link}`}>
                      {shortName(entry.target)}
                    </AppLink>
                    <span className="text-xs text-muted-foreground">{entry.detail === "" ? entry.kind : entry.detail}</span>
                  </li>
                ))}
              </ul>
            )}
          </More>
        )}
      </Fact>
      {explain.key_symbols.length === 0 ? null : (
        <Fact label="Most used classes">
          <ul className="text-sm">
            {explain.key_symbols.slice(0, 5).map((hit) => (
              <li key={`${hit.id}:${hit.path}`} className="py-0.5">
                <AppLink href={classHref(hit.id)} title={hit.path} className={`font-mono text-[0.8125rem] break-all ${link}`}>
                  {hit.id.split(".").slice(-2).join(".")}
                </AppLink>
              </li>
            ))}
          </ul>
        </Fact>
      )}
    </>
  );
}

function Evidence({ text }: { text: string }) {
  return <span className="font-mono text-xs break-all text-muted-foreground">{text}</span>;
}

/** Each difference says which side has it; the code is the side to believe. */
function Differences({ rows }: { rows: Disagreement[] }) {
  return (
    <div id="disagreements" className="mt-4 scroll-mt-6">
      <h3 className="mb-1 font-semibold">Where the docs and the code disagree</h3>
      <p className="mb-2 text-sm text-muted-foreground">The code is taken as true.</p>
      <ul aria-label="Disagreements" className="slab divide-y text-sm">
        {rows.map((row) => (
          <li key={`${row.side}:${row.what}:${row.evidence}`} className="flex flex-wrap items-baseline gap-x-2 px-3 py-2">
            <Flag>{row.side === "code" ? "In the code, not in the docs" : "In the docs, not in the code"}</Flag>
            <span>{row.what}</span>
            <Evidence text={row.evidence} />
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The written intent beside the live code facts, then where the two part ways. */
export function WrittenAndCode({ id, explain, differences }: { id: string; explain: Explain | undefined; differences: Disagreement[] }) {
  return (
    <>
      <div className="grid gap-x-12 gap-y-4 md:grid-cols-2">
        <div role="group" aria-label="Written in the docs" className="min-w-0">
          <h3 className="mb-2 font-semibold">Written in the docs</h3>
          <Intent id={id} />
        </div>
        <div role="group" aria-label="Live from the code" className="min-w-0">
          <h3 className="mb-2 font-semibold">Live from the code</h3>
          <CodeFacts explain={explain} />
        </div>
      </div>
      {differences.length === 0 ? null : <Differences rows={differences} />}
    </>
  );
}
