import type { ItemView } from "@/api/types/ItemView";
import type { ListRow } from "@/api/types/ListRow";

/** A list row with the fields a test does not care about filled in. */
export function listRow(id: string, over: Partial<ListRow> = {}): ListRow {
  const numbered = /^(?:tasks|milestones|decisions)\/(.+)$/.exec(id);
  return {
    id,
    short: numbered?.[1] ?? null,
    path: `stone/${id}.md`,
    type: id.split("/")[0] === "docs" ? "doc" : "task",
    status: null,
    priority: null,
    title: id,
    etag: "etag",
    changed_files: [],
    ...over,
  };
}

/** A fresh item as the server returns it; `over` replaces what a test cares about. */
export function itemView(id: string, over: Partial<ItemView> = {}): ItemView {
  const row = listRow(id);
  return {
    id,
    short: row.short,
    path: row.path,
    type: row.type,
    etag: "etag",
    trust: "unverified",
    fields: { title: id },
    sections: [],
    body: "",
    backlinks: [],
    changed_files: [],
    ...over,
  };
}
