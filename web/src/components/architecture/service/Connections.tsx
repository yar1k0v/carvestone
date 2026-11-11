import { AppLink } from "@/components/markdown/AppLink";
import { serviceHref } from "@/components/graph/serviceHref";
import { type ConnectionRow, callsText } from "./model";
import { Empty, Flag, link } from "./parts";

function Row({ row }: { row: ConnectionRow }) {
  const said = [callsText(row), row.phrase].filter(Boolean).join(", ");
  return (
    <li className="flex flex-wrap items-baseline justify-between gap-x-3 py-1.5">
      {row.id === null ? (
        <span className="text-muted-foreground">{row.title}</span>
      ) : (
        <AppLink href={serviceHref(row.id)} className={link}>
          {row.title}
        </AppLink>
      )}
      <span className="flex flex-wrap items-baseline gap-2 text-sm text-muted-foreground">
        {said}
        {row.state === "code-only" ? <Flag>Not in the docs</Flag> : null}
        {row.state === "written-only" ? <Flag>Not found in the code</Flag> : null}
      </span>
    </li>
  );
}

function Side({ title, rows }: { title: string; rows: ConnectionRow[] }) {
  return (
    <div className="min-w-0">
      <h3 className="mb-1 text-sm font-semibold text-muted-foreground">{title}</h3>
      {rows.length === 0 ? (
        <Empty>Nothing found.</Empty>
      ) : (
        <ul>
          {rows.map((row) => (
            <Row key={row.id ?? row.title} row={row} />
          ))}
        </ul>
      )}
    </div>
  );
}

/** Who calls this service and whom it calls, counted in the code, with what the docs say beside it. */
export function Connections({ calledBy, calls }: { calledBy: ConnectionRow[]; calls: ConnectionRow[] }) {
  return (
    <div className="grid gap-x-12 gap-y-4 md:grid-cols-2">
      <Side title="Called by" rows={calledBy} />
      <Side title="Calls" rows={calls} />
    </div>
  );
}
