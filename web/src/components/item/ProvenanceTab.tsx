import type { ItemView } from "@/api/types/ItemView";
import { asStamp, asStamps } from "./stamps";
import { TrustBadge } from "./TrustBadge";

/** Who generated the item and every review that confirmed it. */
export function ProvenanceTab({ item }: { item: ItemView }) {
  const generated = asStamp(item.fields.generated);
  const verified = asStamps(item.fields.verified);
  return (
    <div className="space-y-6 text-sm">
      <p className="flex items-center gap-2">
        Trust <TrustBadge trust={item.trust} />
      </p>
      <section aria-label="Generated">
        <h2 className="mb-2 font-medium">Generated</h2>
        {generated ? (
          <p>
            <span className="font-mono">{generated.by}</span> on {generated.at}
          </p>
        ) : (
          <p className="text-muted-foreground">Not recorded.</p>
        )}
      </section>
      <section aria-label="Verified">
        <h2 className="mb-2 font-medium">Verified</h2>
        {verified.length === 0 ? (
          <p className="text-muted-foreground">Nobody has verified this item yet.</p>
        ) : (
          <ul className="space-y-1">
            {verified.map((stamp, i) => (
              // The same reviewer can sign twice on one day, so the position is part of the key.
              <li key={`${stamp.by}@${stamp.at}#${i}`}>
                <span className="font-mono">{stamp.by}</span> on {stamp.at}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
