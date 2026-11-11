import { useEffect, useMemo, useState } from "react";
import { cn } from "@/lib/utils";
import type { OutlineEntry } from "./pageFrame";

/** A heading this close under the top of the page counts as the one being read. */
const READ_LINE = 120;

/** The id of the section under the reader's eyes, following the page as it scrolls. */
function useCurrent(ids: string[]): string | null {
  const [current, setCurrent] = useState<string | null>(null);
  useEffect(() => {
    const scroller = document.querySelector("main");
    if (scroller === null) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const line = scroller.getBoundingClientRect().top + READ_LINE;
      let found: string | null = null;
      for (const id of ids) {
        const element = document.getElementById(id);
        if (element === null) continue;
        if (element.getBoundingClientRect().top > line) break;
        found = id;
      }
      setCurrent(found ?? ids[0] ?? null);
    };
    const schedule = () => {
      if (frame === 0) frame = requestAnimationFrame(measure);
    };
    measure();
    scroller.addEventListener("scroll", schedule, { passive: true });
    return () => {
      scroller.removeEventListener("scroll", schedule);
      cancelAnimationFrame(frame);
    };
  }, [ids]);
  return current;
}

function Links({ entries }: { entries: OutlineEntry[] }) {
  const ids = useMemo(() => entries.map((entry) => entry.id), [entries]);
  const current = useCurrent(ids);
  return (
    <ul className="text-sm">
      {entries.map((entry) => (
        <li key={entry.id}>
          <a
            href={`#${entry.id}`}
            aria-current={entry.id === current ? "location" : undefined}
            className={cn(
              "block border-l py-1 text-muted-foreground transition-colors hover:text-foreground",
              (entry.level ?? 2) > 2 ? "pl-6" : "pl-3",
              entry.id === current && "border-primary text-primary hover:text-primary",
            )}
          >
            {entry.title}
          </a>
        </li>
      ))}
    </ul>
  );
}

/** The right column on a wide screen. */
export function OnThisPage({ entries }: { entries: OutlineEntry[] }) {
  return (
    <nav aria-label="On this page" className="sticky top-10 max-h-[calc(100dvh-5rem)] w-[220px] shrink-0 self-start overflow-y-auto">
      <p className="mb-2 text-sm font-medium">On this page</p>
      <Links entries={entries} />
    </nav>
  );
}

/** Folded above the content on a narrower screen. */
export function OnThisPageFolded({ entries }: { entries: OutlineEntry[] }) {
  return (
    <nav aria-label="On this page" className="mb-6">
      <details className="rounded-lg border px-3 py-2">
        <summary className="cursor-pointer text-sm font-medium">On this page</summary>
        <div className="mt-2">
          <Links entries={entries} />
        </div>
      </details>
    </nav>
  );
}
