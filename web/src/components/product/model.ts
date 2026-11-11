import type { DesignDoc } from "@/api/types/DesignDoc";
import type { Feature } from "@/api/types/Feature";
import { bodyLines } from "@/components/docs/bodyLines";

/** `/features/<path>` for the doc `docs/<path>`. */
export function featureHref(id: string): string {
  return `/features/${id.replace(/^docs\//, "")}`;
}

/** The doc behind `/features/<path>`. */
export function featureDocId(target: string): string {
  return target.startsWith("docs/") ? target : `docs/${target}`;
}

/** Features whose name, status or platforms hold the text. */
export function filterFeatures(features: Feature[], text: string): Feature[] {
  const wanted = text.trim().toLowerCase();
  if (wanted === "") return features;
  return features.filter((feature) => [feature.title, feature.status ?? "", ...feature.platforms].some((value) => value.toLowerCase().includes(wanted)));
}

/** Which features the chips above the table keep. */
export type Scope = "all" | "work" | "stale";

export function inScope(feature: Feature, scope: Scope): boolean {
  if (scope === "work") return feature.open_tasks + feature.inferred_open_tasks > 0;
  return scope === "stale" ? feature.stale : true;
}

/** One group of people a product serves. */
export interface Audience {
  name: string;
  text: string;
}

const AUDIENCE_HEADING = /^(who (it|this|we) (serves?|is for)|who .* for|audiences?|segments?|customers?|target (users|customers|audience))\b/i;
const AUDIENCE_LINE = /\b(serves?|segments?|audiences?|customers?)\b/i;
const SUBHEADING = /^\s{0,3}#{2,6}\s/;
const LIST_ITEM = /^\s*(?:[-*+]|\d+[.)])\s+(.*)$/;
const INDENTED = /^\s+\S/;

/** `**Name** — what it is`, `**Name**: what it is` or plain words. */
function audienceOfItem(item: string): Audience {
  const bold = /^\*\*(.+?)\*\*\s*[—–:-]?\s*(.*)$/.exec(item);
  const clean = (text: string) => text.replace(/\*\*|`/g, "").trim();
  if (bold?.[1] !== undefined) return { name: clean(bold[1]).replace(/:$/, ""), text: clean(bold[2] ?? "") };
  return { name: clean(item), text: "" };
}

/** The list items in a run of lines, with their wrapped lines joined on. */
function listItems(lines: string[]): string[] {
  const out: string[] = [];
  for (const line of lines) {
    const item = LIST_ITEM.exec(line)?.[1];
    const last = out.length - 1;
    if (item !== undefined) out.push(item.trim());
    else if (last >= 0 && INDENTED.test(line)) out[last] = `${out[last]} ${line.trim()}`;
  }
  return out;
}

/** Whom the overview says the product serves: a section for it, else a list the opening text introduces. */
export function audienceOf(body: string): Audience[] {
  const lines = bodyLines(body).filter((line) => !line.fenced);
  const headed = lines.filter((line) => line.section !== null && AUDIENCE_HEADING.test(line.section.title));
  if (headed.length > 0) {
    const text = headed.slice(1).map((line) => line.text);
    const items = listItems(text);
    if (items.length > 0) return items.map(audienceOfItem);
    const paragraph = text.join(" ").trim();
    return paragraph === "" ? [] : [{ name: "", text: paragraph }];
  }
  // The opening is everything before the first section; the page title may sit above it.
  const end = lines.findIndex((line) => SUBHEADING.test(line.text));
  const opening = (end < 0 ? lines : lines.slice(0, end)).map((line) => line.text);
  const intro = opening.findIndex((line) => AUDIENCE_LINE.test(line) && !LIST_ITEM.test(line) && !line.trimStart().startsWith("#"));
  if (intro < 0) return [];
  const rest = opening.slice(intro + 1);
  const first = rest.findIndex((line) => line.trim() !== "");
  if (first < 0 || !LIST_ITEM.test(rest[first] ?? "")) return [];
  const blank = rest.findIndex((line, at) => at > first && line.trim() === "");
  return listItems(rest.slice(first, blank < 0 ? undefined : blank)).map(audienceOfItem);
}

/** The tabs of a feature page after the first; the first, behaviour, is the page with no `tab` in its address. */
export type FeatureTab = "technical" | "docs" | "tasks" | "history";

const TABS: FeatureTab[] = ["technical", "docs", "tasks", "history"];

/** The tab an address names, or `undefined` for anything else. */
export function tabOf(value: unknown): FeatureTab | undefined {
  return TABS.find((tab) => tab === value);
}

/** How a technical fact was tied to a feature. */
export type Tie = "linked" | "matched";

export const TIE_LABEL: Record<Tie, string> = { linked: "linked", matched: "matched by name" };

export const TIE_HINT: Record<Tie, string> = {
  linked: "The feature doc links this.",
  matched: "Nothing links the two. They share a word with the feature's title.",
};

const SAME_WORD = new Set(["feature", "features", "service", "services", "system", "systems", "with", "from", "into"]);

function stem(word: string): string {
  if (word.endsWith("ies")) return `${word.slice(0, -3)}y`;
  if (word.endsWith("sses")) return word.slice(0, -2);
  return word.endsWith("s") && !word.endsWith("ss") ? word.slice(0, -1) : word;
}

/** The words of a title or a code name, as one spelling each: `GeofenceRules` gives `geofence`, `rule`. */
export function wordsOf(text: string): Set<string> {
  const split = text.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
  const words = split.split(/[^a-z]+/).filter((word) => word.length >= 4 && !SAME_WORD.has(word));
  return new Set(words.map(stem));
}

function shares(words: Set<string>, text: string): boolean {
  return [...wordsOf(text)].some((word) => words.has(word));
}

export interface Tied<T> {
  value: T;
  tie: Tie;
}

/** Stronger ties first, each keeping its order. */
function strongestFirst<T>(rows: Tied<T>[]): Tied<T>[] {
  return [...rows.filter((row) => row.tie !== "matched"), ...rows.filter((row) => row.tie === "matched")];
}

/** The flows of a service that the feature links, then those that share a word with its title. */
export function flowsOfFeature(docs: DesignDoc[], title: string, linked: Set<string>): Tied<DesignDoc>[] {
  const words = wordsOf(title);
  const out: Tied<DesignDoc>[] = [];
  for (const doc of docs) {
    if (doc.kind !== "flow") continue;
    if (linked.has(doc.id)) out.push({ value: doc, tie: "linked" });
    else if (shares(words, doc.title)) out.push({ value: doc, tie: "matched" });
  }
  return strongestFirst(out);
}
