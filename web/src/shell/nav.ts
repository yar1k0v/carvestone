import type { DocRow } from "@/api/types/DocRow";
import { byName, groupCounts, readable } from "@/components/docs/library";
import { useDocRows } from "@/components/docs/useDocs";
import { serviceHref } from "@/components/graph/serviceHref";
import { useHld } from "@/components/graph/useDesign";
import { hrefFor } from "@/components/markdown/links";

export interface NavEntry {
  label: string;
  href: string;
  /** How many items sit behind the entry, shown at its right. */
  count?: number;
  /** Active on this address only, not on the pages below it. */
  exact?: boolean;
}

export type Space = "now" | "product" | "system" | "library";

export const SPACES: { space: Space; label: string; href: string }[] = [
  { space: "now", label: "Now", href: "/" },
  { space: "product", label: "Product", href: "/product" },
  { space: "system", label: "System", href: "/architecture" },
  { space: "library", label: "Library", href: "/docs" },
];

const NOW: NavEntry[] = [
  { label: "Today", href: "/", exact: true },
  { label: "Board", href: "/board" },
  { label: "Milestones", href: "/milestones" },
  { label: "Plans", href: "/plans" },
  { label: "Shipped", href: "/activity" },
];

/** The space a page belongs to; a page no space claims reads as Now. */
export function spaceOf(pathname: string): Space {
  const root = pathname.split("/")[1] ?? "";
  if (root === "product" || root === "features") return "product";
  if (root === "architecture" || root === "code") return "system";
  if (root === "docs") return "library";
  return "now";
}

/** An entry whose `href` holds a query is active on that exact address; the bare library only when no query is open. */
export function isActive(pathname: string, entry: NavEntry, search = ""): boolean {
  if (entry.href.includes("?")) return `${pathname}${search}` === entry.href;
  if (entry.href === "/docs" && entry.exact === true) return pathname === "/docs" && search === "";
  return pathname === entry.href || (entry.exact !== true && pathname.startsWith(`${entry.href}/`));
}

/** Docs of the family shown beside a doc being read; a family can hold hundreds. */
const FAMILY_MAX = 30;

function libraryNav(pathname: string, rows: DocRow[]): NavEntry[] {
  const readableRows = readable(rows, false);
  const all: NavEntry = { label: "All documents", href: "/docs", exact: true, count: readableRows.length };
  if (pathname.startsWith("/docs/")) {
    const open = rows.find((row) => hrefFor(row.id) === pathname);
    const family = open === undefined ? [] : byName(readableRows.filter((row) => row.group === open.group)).slice(0, FAMILY_MAX);
    return [{ label: all.label, href: all.href, exact: true }, ...family.map((row) => ({ label: row.title, href: hrefFor(row.id) }))];
  }
  return [
    { label: "Decisions", href: "/docs?shelf=decisions" },
    { label: "Plans", href: "/docs?shelf=plans" },
    all,
    ...groupCounts(readableRows).map((group) => ({ label: group.label, href: `/docs?group=${encodeURIComponent(group.value)}`, count: group.count })),
  ];
}

/** The sidebar entries of one space. System lists what the high-level design knows, so it fits any project. */
export function useSpaceNav(space: Space, pathname = ""): NavEntry[] {
  const hld = useHld();
  // Docs are read only where the Library shows them.
  const docs = useDocRows(space === "library");
  if (space === "now") return NOW;
  if (space === "system") {
    return [
      { label: "Overview", href: "/architecture", exact: true },
      ...(hld.data?.containers ?? []).map((part) => ({ label: part.title, href: serviceHref(part.id) })),
    ];
  }
  if (space === "library") return libraryNav(pathname, docs.data ?? []);
  return [];
}

/** The name in the top bar: the system the design describes, when there is exactly one. */
export function useBrand(): string {
  const hld = useHld();
  // Blank until the design answers, so a project never flashes the fallback name.
  if (hld.isPending) return "";
  const systems = hld.data?.systems ?? [];
  return systems.length === 1 ? (systems[0]?.title ?? "Carvestone") : "Carvestone";
}
