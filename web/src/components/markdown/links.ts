import type { ListRow } from "@/api/types/ListRow";

/** An item a link points at, and where the app shows it. */
export interface Resolved {
  id: string;
  href: string;
  /** What a person calls it; `null` when it has no title. */
  title: string | null;
}

export interface Resolver {
  /** False until the item list has loaded; links are not marked broken before that. */
  ready: boolean;
  /** A `[[target]]`: any spelling the engine accepts (`CS-2`, `tasks/cs-002`, `docs/prd#anchor`). */
  wiki(target: string): Resolved | null;
  /** A relative `.md` link, resolved from the item that holds it. */
  md(href: string, fromId: string | null): Resolved | null;
}

/** The task page, from a path id (`tasks/CS-2`) or a short id (`CS-2`). */
export function taskHref(id: string): string {
  return `/tasks/${id.replace(/^tasks\//, "")}`;
}

/** Docs live in the docs site, tasks on the task page, everything else on the item page. */
export function hrefFor(id: string, anchor?: string | null): string {
  const base = id.startsWith("docs/") ? `/docs/${id.slice("docs/".length)}` : id.startsWith("tasks/") ? taskHref(id) : `/items/${id}`;
  return anchor ? `${base}#${anchor}` : base;
}

export function codeHref(target: string): string {
  return `/code?target=${encodeURIComponent(target)}`;
}

/** `../x.md` from `docs/a`, as the engine joins it; `null` when it climbs out of the store. */
export function joinTarget(fromId: string, target: string): string | null {
  const slash = fromId.lastIndexOf("/");
  const parts = (slash < 0 ? "" : fromId.slice(0, slash)).split("/").filter((s) => s !== "");
  for (const segment of target.split("/")) {
    if (segment === "." || segment === "") continue;
    if (segment === "..") {
      if (parts.pop() === undefined) return null;
    } else {
      parts.push(segment);
    }
  }
  const joined = parts.join("/");
  return joined.endsWith(".md") ? joined.slice(0, -3) : joined;
}

const NUMBERED = /^(?:(?:tasks|milestones|decisions)\/)?([a-z]+)-0*(\d+)$/i;

/** `CS-007`, `cs-7` and `tasks/CS-7` share a key, as the engine accepts them as one item. */
function looseKey(text: string): string {
  const cleaned = text.replace(/^\//, "").replace(/\.md$/, "");
  const numbered = NUMBERED.exec(cleaned);
  return numbered ? `${numbered[1]}-${numbered[2]}`.toLowerCase() : cleaned.toLowerCase();
}

function splitAnchor(text: string): [string, string | null] {
  const hash = text.indexOf("#");
  if (hash < 0) return [text, null];
  const anchor = text.slice(hash + 1).toLowerCase();
  return [text.slice(0, hash), anchor === "" ? null : anchor];
}

export function buildResolver(rows: ListRow[], ready = true): Resolver {
  const byKey = new Map<string, string>();
  const titles = new Map(rows.map((row) => [row.id, row.title]));
  for (const row of rows) {
    byKey.set(looseKey(row.id), row.id);
    if (row.short !== null) byKey.set(looseKey(row.short), row.id);
  }
  const lookup = (path: string, anchor: string | null): Resolved | null => {
    const id = byKey.get(looseKey(path.trim()));
    return id === undefined ? null : { id, href: hrefFor(id, anchor), title: titles.get(id) ?? null };
  };
  return {
    ready,
    wiki(target) {
      const [path, anchor] = splitAnchor(target.trim());
      return lookup(path, anchor);
    },
    md(href, fromId) {
      const [path, anchor] = splitAnchor(href);
      const joined = fromId === null ? path : joinTarget(fromId, path);
      return joined === null ? null : lookup(joined, anchor);
    },
  };
}
