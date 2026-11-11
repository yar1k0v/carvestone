import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { DocsLibrary, type LibrarySearch } from "@/components/docs/DocsLibrary";

function text(value: unknown): string | undefined {
  return typeof value === "string" && value !== "" ? value : undefined;
}

export const Route = createFileRoute("/docs/")({
  validateSearch: (search: Record<string, unknown>): LibrarySearch => ({
    ...(text(search.q) === undefined ? {} : { q: text(search.q) }),
    ...(text(search.group) === undefined ? {} : { group: text(search.group) }),
    ...(search.shelf === "decisions" || search.shelf === "plans" ? { shelf: search.shelf } : {}),
    ...(text(search.part) === undefined ? {} : { part: text(search.part) }),
    ...(search.sort === "name" ? { sort: "name" as const } : {}),
    ...(search.templates === true || search.templates === "true" ? { templates: true } : {}),
  }),
  component: DocsHome,
});

function DocsHome() {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  return <DocsLibrary search={search} onSearch={(next) => void navigate({ search: next, replace: true })} />;
}
