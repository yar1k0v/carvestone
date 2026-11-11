import type { MouseEvent, ReactNode } from "react";
import type { ListRow } from "@/api/types/ListRow";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { usePeek } from "./peek";

/** A link to a task: a plain click opens the task view over the page, a modified click still opens a tab. */
export function TaskLink({ row, className, children }: { row: ListRow; className?: string; children: ReactNode }) {
  const peek = usePeek();
  const href = hrefFor(row.id);
  if (peek === null) {
    return (
      <AppLink href={href} className={className}>
        {children}
      </AppLink>
    );
  }
  const click = (event: MouseEvent) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    peek(row);
  };
  return (
    <AppLink href={href} className={className} onClick={click}>
      {children}
    </AppLink>
  );
}
