import { Fragment } from "react";
import type { JsonValue } from "@/api/types/serde_json/JsonValue";
import { AppLink } from "@/components/markdown/AppLink";
import type { Resolver } from "@/components/markdown/links";
import { dayOf } from "./stamps";

/** Fields whose values name other items. */
const RELATIONS = new Set(["links", "depends_on", "components", "uses", "milestone", "plan", "supersedes", "parent"]);

/** Shown in the header or on the Provenance tab instead. */
const SKIPPED = new Set(["title", "generated", "verified"]);

/** Dates that may hold a timestamp; the table shows the day only. */
const DAYS = new Set(["created", "closed", "stale_after"]);

function Scalar({ name, value, resolver }: { name: string; value: JsonValue; resolver: Resolver }) {
  if (typeof value === "string" && RELATIONS.has(name)) {
    const found = resolver.wiki(value);
    if (found) {
      return (
        <AppLink href={found.href} className="text-primary underline underline-offset-2">
          {value}
        </AppLink>
      );
    }
    if (resolver.ready) {
      return (
        <span data-broken="true" title={`${value} was not found`} className="text-destructive underline decoration-dotted">
          {value}
        </span>
      );
    }
  }
  if (value === null) return <span className="text-muted-foreground">none</span>;
  if (typeof value === "object") return <code className="font-mono text-xs">{JSON.stringify(value)}</code>;
  return <>{typeof value === "string" && DAYS.has(name) ? dayOf(value) : String(value)}</>;
}

function Value({ name, value, resolver }: { name: string; value: JsonValue; resolver: Resolver }) {
  if (!Array.isArray(value)) return <Scalar name={name} value={value} resolver={resolver} />;
  return (
    <>
      {value.map((entry, i) => (
        // A list can repeat a value; its position keeps the keys apart.
        <Fragment key={`${String(entry)}#${i}`}>
          {i > 0 ? ", " : null}
          <Scalar name={name} value={entry} resolver={resolver} />
        </Fragment>
      ))}
    </>
  );
}

/** Every frontmatter field of the item; relation fields are links. */
export function FieldTable({ fields, resolver }: { fields: Record<string, JsonValue>; resolver: Resolver }) {
  const rows = Object.entries(fields).filter(([name]) => !SKIPPED.has(name));
  if (rows.length === 0) return null;
  return (
    <table className="w-full text-sm">
      <tbody>
        {rows.map(([name, value]) => (
          <tr key={name} className="border-t first:border-t-0">
            <th scope="row" className="w-40 py-1.5 pr-3 text-left align-top font-mono text-xs font-normal text-muted-foreground">
              {name}
            </th>
            <td className="py-1.5">
              <Value name={name} value={value} resolver={resolver} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
