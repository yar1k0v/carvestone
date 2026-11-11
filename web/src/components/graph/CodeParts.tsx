import { plainNote } from "@/components/architecture/service/model";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { ambiguityOf } from "./useCode";

/** The choices for a name that fits several symbols; picking one loads it. */
export function Candidates({ name, options, onPick }: { name: string; options: string[]; onPick: (option: string) => void }) {
  return (
    <section aria-label="Candidates" className="rounded-md border p-4 text-sm">
      <p className="font-medium">
        <span className="font-mono">{name}</span> matches more than one symbol. Choose one:
      </p>
      <ul className="mt-2 space-y-1">
        {options.map((option) => (
          <li key={option}>
            <button
              type="button"
              onClick={() => onPick(option)}
              className="font-mono text-primary underline underline-offset-2"
            >
              {option}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** A failed code call: the candidate list for an ambiguous name, the engine's sentence otherwise. */
export function CodeError({ error, onPick }: { error: Error; onPick: (option: string, name: string) => void }) {
  const ambiguity = ambiguityOf(error);
  if (ambiguity === null) return <ErrorNotice error={error} />;
  return <Candidates name={ambiguity.name} options={ambiguity.options} onPick={(option) => onPick(option, ambiguity.name)} />;
}

export function ExactnessBadge({ exactness }: { exactness: string | undefined }) {
  const inferred = exactness === "inferred";
  return (
    <span
      className={`rounded-full border px-1.5 py-0.5 text-[11px] ${
        inferred ? "border-dashed text-muted-foreground" : "text-foreground"
      }`}
    >
      {inferred ? "inferred" : "exact"}
    </span>
  );
}

export function Notes({ note, truncated }: { note?: string; truncated: boolean }) {
  return (
    <>
      {note ? <p className="mb-2 text-xs text-muted-foreground">{plainNote(note)}</p> : null}
      {truncated ? <p className="mb-2 text-xs text-muted-foreground">More results exist than are shown.</p> : null}
    </>
  );
}
