import type { ReactNode } from "react";
import type { SymbolEdge } from "@/api/types/SymbolEdge";
import type { SymbolMember } from "@/api/types/SymbolMember";
import type { SymbolSource } from "@/api/types/SymbolSource";
import { ExactnessBadge } from "@/components/graph/CodeParts";
import { sourceOf } from "@/components/graph/sourceOf";
import { AppLink } from "@/components/markdown/AppLink";
import { More } from "@/components/ui/page";
import { classHref } from "./href";
import { kindWord, sourceLines } from "./model";

export const LINK = "text-primary underline-offset-2 hover:underline";
const ROW = "space-y-0.5 px-3 py-2";

/** One part of the page. `id` is the heading's, so "On this page" can point at it. */
export function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="mt-10">
      <h2 id={id} className="mb-3 scroll-mt-6 text-xl font-semibold tracking-tight">
        {title}
      </h2>
      {children}
    </section>
  );
}

export function Muted({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground">{children}</p>;
}

/** Rows of a section, cut to the first few until asked for the rest. */
export function RowList<T>({ rows, limit = 15, children }: { rows: T[]; limit?: number; children: (row: T) => ReactNode }) {
  return (
    <More rows={rows} limit={limit}>
      {(shown) => <ul className="divide-y rounded-lg border text-sm">{shown.map(children)}</ul>}
    </More>
  );
}

export function Row({ children }: { children: ReactNode }) {
  return <li className={ROW}>{children}</li>;
}

function counts(member: SymbolMember): string {
  const said = [member.used_by === 0 ? "" : `used by ${member.used_by}`, member.uses === 0 ? "" : `uses ${member.uses}`];
  return said.filter(Boolean).join(", ");
}

export function MemberList({ members }: { members: SymbolMember[] }) {
  if (members.length === 0) return <Muted>Nothing is declared inside it.</Muted>;
  return (
    <RowList rows={members} limit={30}>
      {(member) => (
        <li key={`${member.id}:${member.lines[0]}`} className={ROW} style={{ paddingLeft: `${0.75 + member.depth}rem` }}>
          <div className="flex flex-wrap items-baseline gap-x-2">
            <AppLink href={classHref(member.id)} className={`font-mono ${LINK}`}>
              {member.name}
            </AppLink>
            <span className="text-xs text-muted-foreground">
              {kindWord(member.kind, member.signature).toLowerCase()}, line {member.lines[0]}
            </span>
            <span className="ml-auto text-xs text-muted-foreground">{counts(member)}</span>
          </div>
          <p className="truncate font-mono text-xs text-muted-foreground">{member.short || member.signature}</p>
          {member.doc === undefined ? null : <p className="line-clamp-2 text-xs">{member.doc}</p>}
        </li>
      )}
    </RowList>
  );
}

/** The symbols on the other end of its links. */
export function EdgeList({ edges, empty }: { edges: SymbolEdge[]; empty: string }) {
  if (edges.length === 0) return <Muted>{empty}</Muted>;
  return (
    <RowList rows={edges}>
      {({ hit, through }) => (
        <Row key={`${hit.id}|${hit.kind}|${hit.path}:${hit.lines[0]}`}>
          <div className="flex flex-wrap items-baseline gap-x-2">
            <AppLink href={classHref(hit.id)} className={`break-all font-mono ${LINK}`}>
              {hit.id}
            </AppLink>
            <span className="text-xs text-muted-foreground">{hit.kind}</span>
            {hit.exactness === "inferred" ? <ExactnessBadge exactness="inferred" /> : null}
          </div>
          {through.length > 0 ? <p className="text-xs text-muted-foreground">through {through.join(", ")}</p> : null}
          <p className="break-all font-mono text-xs text-muted-foreground">{sourceOf(hit)}</p>
        </Row>
      )}
    </RowList>
  );
}

/** The code itself, with the numbers its lines have in the file. */
export function SourceText({ source }: { source: SymbolSource }) {
  return (
    <>
      <div tabIndex={0} role="group" aria-label="Source code" className="max-h-[70vh] overflow-auto rounded-lg border bg-muted/40">
        <table className="font-mono text-xs leading-5">
          <tbody>
            {sourceLines(source.text, source.start_line).map((line) => (
              <tr key={line.number}>
                <td aria-hidden="true" className="sticky left-0 bg-muted px-2 text-right text-muted-foreground tabular-nums select-none">
                  {line.number}
                </td>
                <td className="px-3 whitespace-pre">{line.text}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {source.truncated ? <p className="mt-2 text-xs text-muted-foreground">Too long to show in full: it goes on to line {source.end_line}.</p> : null}
    </>
  );
}

export interface Related {
  id: string;
  href: string;
  label: string;
  how: string;
}

/** Docs, tasks and decisions tied to this code, each saying how. */
export function RelatedList({ rows, empty }: { rows: Related[]; empty: string }) {
  if (rows.length === 0) return <p className="mt-3 text-sm text-muted-foreground">{empty}</p>;
  return (
    <ul className="mt-3 space-y-1 text-sm">
      {rows.map((row) => (
        <li key={row.id} className="flex flex-wrap gap-x-2">
          <AppLink href={row.href} className={LINK}>
            {row.label}
          </AppLink>
          <span className="text-muted-foreground">{row.how}</span>
        </li>
      ))}
    </ul>
  );
}
