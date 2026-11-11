import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";

/** The suggestion in the engine's `X not found. Did you mean Y?`, if it made one. */
function suggestion(message: string): string | null {
  return /Did you mean (\S+?)\?/.exec(message)?.[1] ?? null;
}

/** A failed request, in the engine's own words; a did-you-mean becomes a link. */
export function ErrorNotice({ error }: { error: Error }) {
  const suggested = suggestion(error.message);
  return (
    <div role="alert" className="rounded-md border border-destructive/50 p-4 text-sm">
      <p className="font-medium text-destructive">{error.message}</p>
      {suggested ? (
        <p className="mt-2">
          Open{" "}
          <AppLink href={hrefFor(suggested)} className="text-primary underline underline-offset-2">
            {suggested}
          </AppLink>{" "}
          instead?
        </p>
      ) : null}
    </div>
  );
}
