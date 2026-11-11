import { isStaleNote } from "./model";
import type { UseQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { classHref } from "@/components/architecture/symbol/href";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { Loading } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";

export const link = "text-primary underline underline-offset-2";

/** One section of the page, with the heading "On this page" jumps to. */
export function Block({ id, title, lead, children }: { id: string; title: string; lead?: string; children: ReactNode }) {
  return (
    <section id={id} aria-label={title} className="scroll-mt-6 border-t py-7">
      <h2 className="mb-3 text-[1.375rem] leading-7 font-semibold">{title}</h2>
      {lead === undefined ? null : <p className="mb-3 text-sm text-muted-foreground">{lead}</p>}
      {children}
    </section>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground">{children}</p>;
}

/** What the server adds to one answer, such as the code index still being built. Out-of-date links show once per page. */
export function Note({ children }: { children: string | undefined }) {
  return children === undefined || isStaleNote(children) ? null : <p className="mb-2 text-xs text-muted-foreground">{children}</p>;
}

/** Where a thing is in the code: `file:line`, opening what sits there. */
export function CodeLink({ file, line }: { file: string; line?: number | undefined }) {
  const place = line === undefined ? file : `${file}:${line}`;
  return (
    <AppLink href={classHref(place)} className="font-mono text-xs break-all text-muted-foreground hover:text-foreground hover:underline">
      {place}
    </AppLink>
  );
}

/** A short word beside a row: what it is, in a border. */
export function Tag({ children, title, className }: { children: ReactNode; title?: string; className?: string }) {
  return (
    <span title={title} className={cn("inline-block rounded-md border px-1.5 text-xs leading-[1.125rem] whitespace-nowrap text-muted-foreground", className)}>
      {children}
    </span>
  );
}

/** Marks where the written design and the code differ. */
export function Flag({ children }: { children: ReactNode }) {
  return <Tag className="border-st-blocked/35 bg-st-blocked/10 text-st-blocked">{children}</Tag>;
}

/**
 * The three states of a read: waiting, failed and answered. `quiet` shows a failure as a plain
 * sentence, for a read that fails in the normal way, such as a service with no code yet.
 */
export function Answer<T>({ query, label, quiet = false, children }: { query: UseQueryResult<T>; label: string; quiet?: boolean; children: (data: T) => ReactNode }) {
  if (query.isPending) return <Loading lines={3} label={label} className="max-w-xl" />;
  if (query.isError) return quiet ? <Empty>{query.error.message}</Empty> : <ErrorNotice error={query.error} />;
  return <>{children(query.data)}</>;
}
