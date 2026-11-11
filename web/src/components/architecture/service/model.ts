import type { ArchGraph } from "@/api/types/ArchGraph";
import type { Connection } from "@/api/types/Connection";
import type { Dependency } from "@/api/types/Dependency";
import type { DriftFinding } from "@/api/types/DriftFinding";
import type { Explain } from "@/api/types/Explain";
import type { Hld } from "@/api/types/Hld";
import type { ListRow } from "@/api/types/ListRow";
import type { Module } from "@/api/types/Module";
import type { Part } from "@/api/types/Part";

export type TitleOf = (id: string) => string;

/** Tasks in progress that name this service by its title. */
export function runningOn(part: Part, tasks: ListRow[]): ListRow[] {
  return tasks.filter((task) => task.status === "in-progress" && (task.services ?? []).includes(part.title));
}

/** Every difference between docs and code on a service or inside it. */
export function driftRows(id: string, graph: ArchGraph | undefined): DriftFinding[] {
  // The `/` keeps `arch/api-gateway` from counting for `arch/api`.
  const within = (other: string | undefined) => other === id || other?.startsWith(`${id}/`) === true;
  return (graph?.drift ?? []).filter((row) => within(row.from) || within(row.to));
}

const STALE_LINKS = /^native edges are out of date for (\d+) files? \(([^)]*)\)/;

export function isStaleNote(note: string): boolean {
  return STALE_LINKS.test(note);
}

/** A note in words for this page: the server's says what to run, which helps only at a terminal. */
export function plainNote(note: string): string {
  const stale = STALE_LINKS.exec(note);
  if (stale === null) return note;
  return `The compiler's view of ${stale[1]} ${stale[1] === "1" ? "file" : "files"} in ${stale[2]} is out of date, so some links may be missing.`;
}

/** Titles of everything the design and the code graph name; an unknown id reads as its last word. */
export function titlesOf(hld: Hld | undefined, graph: ArchGraph | undefined): TitleOf {
  const titles = new Map<string, string>();
  for (const node of graph?.nodes ?? []) {
    if (node.title !== null) titles.set(node.id, node.title);
  }
  for (const part of [...(hld?.systems ?? []), ...(hld?.containers ?? [])]) titles.set(part.id, part.title);
  return (id) => titles.get(id) ?? id.split("/").pop() ?? id;
}

/** What the design or the code graph knows about an element, as one shape. */
export function findPart(id: string, hld: Hld | undefined, graph: ArchGraph | undefined): Part | undefined {
  const known = [...(hld?.systems ?? []), ...(hld?.containers ?? [])].find((part) => part.id === id);
  if (known !== undefined) return known;
  const node = graph?.nodes.find((entry) => entry.id === id);
  if (node === undefined) return undefined;
  return { id: node.id, title: node.title ?? node.id, type: node.type, ...(node.parent === null ? {} : { parent: node.parent }) };
}

/** Every written dependency, once: the design's connections, then the `uses` lists it leaves out. */
export function writtenLinks(hld: Hld | undefined, graph: ArchGraph | undefined): Connection[] {
  const seen = new Map<string, Connection>();
  for (const link of [...(hld?.connections ?? []), ...(graph?.uses ?? [])]) {
    const key = `${link.from}>${link.to}`;
    if (!seen.has(key)) seen.set(key, link);
  }
  return [...seen.values()];
}

/** An element id names what sits inside it as a longer path, so code on a child counts for the parent. */
function sameOrInside(a: string, b: string): boolean {
  return a === b || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
}

export type ConnectionState =
  /** Written, and the code could not be read to check it. */
  | "written"
  /** Written and found in the code. */
  | "agreed"
  /** In the code only. */
  | "code-only"
  /** Written only. */
  | "written-only";

export interface ConnectionRow {
  /** `null` for code no service owns. */
  id: string | null;
  title: string;
  calls: number;
  inferredOnly: boolean;
  phrase?: string;
  state: ConnectionState;
}

const UNOWNED = "(unowned)";

/**
 * One side of a service's connections with the written and the code answers side by side. `code` is
 * undefined while the code cannot be read; nothing is flagged then.
 */
export function connectionRows(side: "calls" | "called-by", id: string, links: Connection[], code: Dependency[] | undefined, titleOf: TitleOf): ConnectionRow[] {
  const written = links.flatMap((link) => {
    const other = side === "calls" ? (link.from === id ? link.to : null) : link.to === id ? link.from : null;
    return other === null ? [] : [{ other, phrase: link.description }];
  });
  const phraseOf = (phrase: string | undefined) => (phrase === undefined ? {} : { phrase });
  if (code === undefined) {
    return written.map(({ other, phrase }) => ({ id: other, title: titleOf(other), calls: 0, inferredOnly: false, state: "written", ...phraseOf(phrase) }));
  }
  const matched = new Set<string>();
  const rows: ConnectionRow[] = [];
  for (const dep of code) {
    const calls = dep.exact + dep.inferred;
    const inferredOnly = dep.exact === 0;
    if (dep.component === UNOWNED) {
      rows.push({ id: null, title: "Code outside any service", calls, inferredOnly, state: "agreed" });
      continue;
    }
    if (sameOrInside(dep.component, id)) continue;
    const found = written.find(({ other }) => sameOrInside(other, dep.component));
    if (found !== undefined) matched.add(found.other);
    rows.push({
      id: dep.component,
      title: titleOf(dep.component),
      calls,
      inferredOnly,
      state: found === undefined ? "code-only" : "agreed",
      ...phraseOf(found?.phrase),
    });
  }
  for (const { other, phrase } of written) {
    if (!matched.has(other)) rows.push({ id: other, title: titleOf(other), calls: 0, inferredOnly: false, state: "written-only", ...phraseOf(phrase) });
  }
  return rows.sort((a, b) => b.calls - a.calls || a.title.localeCompare(b.title));
}

/** The tabs of a service page after the first; the first, Inside, is the page with no `tab` in its address. */
export type ServiceTab = "domain" | "flows" | "commits";

const SERVICE_TABS: ServiceTab[] = ["domain", "flows", "commits"];

/** The tab an address names, or `undefined` for anything else. */
export function serviceTabOf(value: unknown): ServiceTab | undefined {
  return SERVICE_TABS.find((tab) => tab === value);
}

/** The four facts at the top of a service page. `running` counts the tasks in progress on it. */
export function keyFacts(part: Part, calledBy: ConnectionRow[], calls: ConnectionRow[], running: number): { label: string; value: string }[] {
  const callers = calledBy.flatMap((row) => (row.id === null ? [] : [row.title]));
  const more = callers.length - 3;
  const heaviest = calls.find((row) => row.calls > 0 && row.id !== null);
  return [
    { label: "Called by", value: callers.length === 0 ? "None found" : `${callers.slice(0, 3).join(", ")}${more > 0 ? ` and ${more} more` : ""}` },
    { label: "Uses most", value: heaviest === undefined ? "None found" : `${heaviest.title}, ${heaviest.calls} ${heaviest.calls === 1 ? "call" : "calls"}` },
    { label: "Stack", value: part.technology ?? "Not written" },
    { label: "Running", value: running === 0 ? "None" : `${running} ${running === 1 ? "task" : "tasks"}` },
  ];
}

/** `3 calls found`, or nothing when no code was counted. */
export function callsText(row: { calls: number; inferredOnly: boolean }): string {
  if (row.calls === 0) return "";
  return `${row.calls} ${row.calls === 1 ? "call" : "calls"} ${row.inferredOnly ? "inferred" : "found"}`;
}

export interface ModuleLink {
  folder: string;
  name: string;
  count: number;
}

export interface ModuleRow {
  folder: string;
  name: string;
  files: number;
  calls: ModuleLink[];
  calledBy: ModuleLink[];
}

export interface ModuleGroup {
  title: string;
  modules: ModuleRow[];
}

/** The path all folders sit under. One of them may be that path itself, holding the files at the top. */
function sharedPath(folders: string[]): string {
  const [first, ...rest] = folders.map((folder) => folder.split("/"));
  if (first === undefined) return "";
  let keep = first.length;
  for (const parts of rest) {
    let same = 0;
    while (same < keep && parts[same] === first[same]) same += 1;
    keep = same;
  }
  return first.slice(0, keep).join("/");
}

/** The deepest of `folders` that holds a file: the one it counts for. */
function ownerOf(file: string, folders: string[]): string | undefined {
  const holding = folders.filter((folder) => file.startsWith(`${folder}/`));
  return holding.sort((a, b) => b.length - a.length)[0];
}

/**
 * The folders of a service as modules, by the part each plays: where outside work comes in, what the
 * others share, and what stands alone. Worked out from the code, so it fits any language.
 */
export function moduleGroups(explain: Explain, serviceTitle: string): ModuleGroup[] {
  const folders = explain.folders ?? [];
  const edges = explain.folder_edges ?? [];
  const names = folders.map((row) => row.folder);
  const shared = sharedPath(names);
  const nameOf = (folder: string) => (folder === shared ? (folder.split("/").pop() ?? folder) : folder.slice(shared === "" ? 0 : shared.length + 1));
  const link = (folder: string, count: number): ModuleLink => ({ folder, name: nameOf(folder), count });
  const byCount = (a: ModuleLink, b: ModuleLink) => b.count - a.count;
  const rows: ModuleRow[] = folders.map((row) => ({
    folder: row.folder,
    name: nameOf(row.folder),
    files: row.files,
    calls: edges.filter((edge) => edge.from === row.folder).map((edge) => link(edge.to, edge.exact + edge.inferred)).sort(byCount),
    calledBy: edges.filter((edge) => edge.to === row.folder).map((edge) => link(edge.from, edge.exact + edge.inferred)).sort(byCount),
  }));
  const entryFolders = new Set(explain.entries.map((entry) => ownerOf(entry.target, names)));
  const holdsEntry = (row: ModuleRow) => entryFolders.has(row.folder);
  const entry = rows.filter(holdsEntry);
  const rest = rows.filter((row) => !holdsEntry(row));
  const groups = [
    { title: "Entry points", modules: entry },
    { title: `Shared inside ${serviceTitle}`, modules: rest.filter((row) => row.calledBy.length > 0) },
    { title: "On their own", modules: rest.filter((row) => row.calledBy.length === 0) },
  ];
  return groups.filter((group) => group.modules.length > 0);
}

/** The modules the index found in a folder, biggest first. A file counts for the deepest of `folders` that holds it. */
export function modulesIn(modules: Module[], folder: string, folders: string[]): Module[] {
  return modules.filter((module) => module.files.some((file) => ownerOf(file, folders) === folder)).sort((a, b) => b.files.length - a.files.length);
}

/** `Auth 3, Options 1`, or what the list being empty means. */
export function linksText(links: ModuleLink[]): string {
  return links.length === 0 ? "nothing inside this service" : links.map((link) => `${link.name} ${link.count}`).join(", ");
}

export interface Disagreement {
  /** `code` when only the code has it: the code is right. `docs` when only the docs claim it. */
  side: "code" | "docs";
  what: string;
  /** Where it shows: a code site or an item and line. */
  evidence: string;
}

/** Every place the written design and the code differ about one service, code first. */
export function disagreements(id: string, graph: ArchGraph | undefined, titleOf: TitleOf): Disagreement[] {
  const all = (graph?.drift ?? [])
    .filter((row) => row.from === id || row.to === id)
    .map((row): Disagreement => {
      const evidence = `${row.path}:${row.line}`;
      if (row.to === undefined) return { side: row.kind === "code" ? "code" : "docs", what: row.message, evidence };
      return row.kind === "code"
        ? { side: "code", what: `${titleOf(row.from)} calls ${titleOf(row.to)}`, evidence }
        : { side: "docs", what: `${titleOf(row.from)} lists ${titleOf(row.to)} as a dependency`, evidence };
    });
  return [...all.filter((row) => row.side === "code"), ...all.filter((row) => row.side === "docs")];
}

/** What the list and the fallback drawing show: the design's containers, else the top elements the code knows. */
export function servicesOf(hld: Hld | undefined, graph: ArchGraph | undefined): Part[] {
  if ((hld?.containers.length ?? 0) > 0) return hld?.containers ?? [];
  const nodes = graph?.nodes ?? [];
  const systems = new Set(nodes.filter((node) => node.type === "system").map((node) => node.id));
  return nodes.flatMap((node) => {
    if (systems.has(node.id) || (node.parent !== null && !systems.has(node.parent))) return [];
    return [{ id: node.id, title: node.title ?? node.id, type: node.type, ...(node.parent === null ? {} : { parent: node.parent }) }];
  });
}

/**
 * Who depends on whom, drawn from the code: each code edge lifted to the services at its ends. A
 * written link no code backs is kept, so a project with no code index still shows its design.
 */
export function codeConnections(services: Part[], graph: ArchGraph | undefined, written: Connection[]): Connection[] {
  const ids = new Set(services.map((part) => part.id));
  const parentOf = new Map((graph?.nodes ?? []).map((node) => [node.id, node.parent]));
  const lift = (id: string): string | null => {
    let at: string | null | undefined = id;
    while (at !== null && at !== undefined && !ids.has(at)) at = parentOf.get(at);
    return at ?? null;
  };
  const counts = new Map<string, { from: string; to: string; edges: number }>();
  for (const edge of graph?.code_edges ?? []) {
    const from = lift(edge.from);
    const to = lift(edge.to);
    if (from === null || to === null || from === to) continue;
    const key = `${from}>${to}`;
    const known = counts.get(key);
    counts.set(key, { from, to, edges: (known?.edges ?? 0) + edge.edges });
  }
  const links: Connection[] = [...counts.values()].map(({ from, to, edges }) => ({ from, to, description: `${edges} ${edges === 1 ? "call" : "calls"}` }));
  const extra = written.filter((link) => ids.has(link.from) && ids.has(link.to) && !counts.has(`${link.from}>${link.to}`));
  return [...links, ...extra];
}
