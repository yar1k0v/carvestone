import { useState } from "react";
import { ago } from "@/components/docs/library";
import { Answer, Empty } from "./parts";
import { useCommits } from "./useService";

/** The newest commits that touched this service's code. */
export function Commits({ id }: { id: string }) {
  const query = useCommits(id);
  const [now] = useState(() => new Date());
  return (
    <Answer query={query} label="Reading the commits...">
      {(events) =>
        events.length === 0 ? (
          <Empty>No commits found under this service's code.</Empty>
        ) : (
          <ul aria-label="Recent commits">
            {events.map((event) => (
              <li key={event.sha} className="flex items-baseline gap-3 py-1.5 text-sm">
                <code className="shrink-0 font-mono text-xs text-muted-foreground">{event.id}</code>
                <span className="min-w-0 flex-1">{event.title}</span>
                <span title={event.at} className="shrink-0 text-xs text-muted-foreground">
                  {ago(event.at.slice(0, 10), now)}
                </span>
              </li>
            ))}
          </ul>
        )
      }
    </Answer>
  );
}
