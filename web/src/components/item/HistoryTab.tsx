import { Loading } from "@/components/ui/skeleton";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { History } from "@/api/types/History";
import type { HistoryEntry } from "@/api/types/HistoryEntry";
import type { Section } from "@/api/types/Section";
import { ErrorNotice } from "./ErrorNotice";

function lineClass(line: string): string {
  if (line.startsWith("+") && !line.startsWith("+++")) return "bg-emerald-500/10 text-emerald-800 dark:text-emerald-300";
  if (line.startsWith("-") && !line.startsWith("---")) return "bg-red-500/10 text-red-800 dark:text-red-300";
  if (line.startsWith("@@")) return "text-sky-700 dark:text-sky-300";
  return "text-muted-foreground";
}

function Diff({ text }: { text: string }) {
  return (
    <pre className="mt-2 overflow-x-auto rounded-md border bg-muted/40 py-2 font-mono text-xs leading-5">
      {text.split("\n").map((line, i) => (
        // The diff is static text; a line's place in it is its only identity.
        <div key={i} className={`px-3 ${lineClass(line)}`}>
          {line || " "}
        </div>
      ))}
    </pre>
  );
}

function Entry({ entry }: { entry: HistoryEntry }) {
  const head = (
    <>
      <span className="font-mono text-xs" title={entry.sha}>
        {entry.sha.slice(0, 7)}
      </span>
      <span className="text-xs text-muted-foreground">{entry.date}</span>
      <span className="text-xs text-muted-foreground">{entry.author}</span>
      <span className="font-medium">{entry.subject}</span>
    </>
  );
  return (
    <li className="py-2">
      {entry.diff ? (
        <details>
          <summary className="flex cursor-pointer flex-wrap items-baseline gap-x-3">{head}</summary>
          <Diff text={entry.diff} />
        </details>
      ) : (
        <div className="flex flex-wrap items-baseline gap-x-3">{head}</div>
      )}
    </li>
  );
}

/** The commits that touched the item, or one of its sections, with their diffs. */
export function HistoryTab({ id, sections }: { id: string; sections: Section[] }) {
  const [anchor, setAnchor] = useState("");
  const params = { section: anchor, diff: true };
  const query = useQuery({
    queryKey: keys.history(id, params),
    queryFn: () => api.get<History>(`/api/history/${id.split("/").map(encodeURIComponent).join("/")}`, params),
  });

  return (
    <div className="text-sm">
      <label className="mb-3 flex items-center gap-2">
        <span className="text-muted-foreground">Section</span>
        <select
          value={anchor}
          onChange={(event) => setAnchor(event.target.value)}
          className="rounded-md border bg-background px-2 py-1"
        >
          <option value="">Whole item</option>
          {sections.map((section) => (
            <option key={section.anchor} value={section.anchor}>
              {section.title}
            </option>
          ))}
        </select>
      </label>
      {query.isPending ? <Loading /> : null}
      {query.isError ? <ErrorNotice error={query.error} /> : null}
      {query.data && query.data.entries.length === 0 ? (
        <p className="text-muted-foreground">No commits touched this yet.</p>
      ) : null}
      {query.data ? (
        <ul className="divide-y">
          {query.data.entries.map((entry) => (
            <Entry key={entry.sha} entry={entry} />
          ))}
        </ul>
      ) : null}
    </div>
  );
}
