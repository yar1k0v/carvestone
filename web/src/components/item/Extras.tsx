import { Loading } from "@/components/ui/skeleton";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ArchGraph } from "@/api/types/ArchGraph";
import type { ChangedFile } from "@/api/types/ChangedFile";
import type { ItemView } from "@/api/types/ItemView";
import type { LibraryRef } from "@/api/types/LibraryRef";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";

const SOURCE: Record<string, string> = { commit: "in a commit", working: "in the working tree" };

/** A task's files: from its `Refs` commits, and while it is in progress the working changes. */
export function ChangedFiles({ files }: { files: ChangedFile[] }) {
  if (files.length === 0) return <p className="text-sm text-muted-foreground">No changed files recorded.</p>;
  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="text-left text-muted-foreground">
          <th className="py-1 pr-3 font-medium">Status</th>
          <th className="py-1 pr-3 font-medium">File</th>
          <th className="py-1 font-medium">Where</th>
        </tr>
      </thead>
      <tbody>
        {files.map((file) => (
          <tr key={`${file.source}:${file.path}`} className="border-t">
            <td className="py-1 pr-3 font-mono">{file.status}</td>
            <td className="py-1 pr-3 font-mono break-all">{file.path}</td>
            <td className="py-1 whitespace-nowrap text-muted-foreground">
              {SOURCE[file.source] ?? file.source}
              {file.sha ? ` ${file.sha.slice(0, 7)}` : ""}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The line `stone get` prints, so a reader knows to look the libraries up. */
export function Libraries({ libraries }: { libraries: LibraryRef[] }) {
  if (libraries.length === 0) return null;
  const list = libraries.map(({ library, version }) => (version ? `${library}@${version}` : library)).join(", ");
  return (
    <p className="rounded-md border bg-muted/40 px-3 py-2 text-sm">
      Libraries (look up in Docs MCP with search_docs): <span className="font-mono">{list}</span>
    </p>
  );
}

/** What the code says a system, container or component depends on. */
function CodeDependencies({ item }: { item: ItemView }) {
  const edges = item.code_dependencies ?? [];
  return (
    <section aria-label="Code dependencies">
      <h2 className="mb-2 text-sm font-medium">Code dependencies</h2>
      {edges.length === 0 ? (
        <p className="text-sm text-muted-foreground">The code shows no dependency on another element.</p>
      ) : (
        <ul className="space-y-1 text-sm">
          {edges.map((edge) => (
            <li key={edge.to} className="flex flex-wrap items-baseline gap-x-2">
              <AppLink href={hrefFor(edge.to)} className="text-primary underline underline-offset-2">
                {edge.to}
              </AppLink>
              <span className="text-muted-foreground">
                {edge.edges} {edge.edges === 1 ? "edge" : "edges"}, first at{" "}
                <span className="font-mono">
                  {edge.first[0]}:{edge.first[1]}
                </span>
                {edge.inferred_only ? ", inferred only" : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** Where the curated `uses` and `code` of an element disagree with the code. */
function Drift({ item }: { item: ItemView }) {
  const query = useQuery({ queryKey: keys.arch(), queryFn: () => api.get<ArchGraph>("/api/arch/graph") });
  const findings = (query.data?.drift ?? []).filter((problem) => problem.path === item.path);
  return (
    <section aria-label="Drift">
      <h2 className="mb-2 text-sm font-medium">Drift</h2>
      {query.isError ? (
        <p className="text-sm text-destructive">{query.error.message}</p>
      ) : query.isPending ? (
        <Loading />
      ) : findings.length === 0 ? (
        <p className="text-sm text-muted-foreground">The curated links and the code agree.</p>
      ) : (
        <ul className="space-y-1 text-sm">
          {findings.map((problem) => (
            <li key={`${problem.line}:${problem.rule}:${problem.message}`}>
              <span className="font-mono text-xs text-muted-foreground">
                line {problem.line}, {problem.rule}
              </span>{" "}
              {problem.message}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The extras of an architecture element: code dependencies and drift. */
export function ArchExtras({ item }: { item: ItemView }) {
  return (
    <div className="space-y-6">
      <CodeDependencies item={item} />
      <Drift item={item} />
    </div>
  );
}
