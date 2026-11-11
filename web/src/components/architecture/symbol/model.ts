import type { Module } from "@/api/types/Module";
import type { SymbolMember } from "@/api/types/SymbolMember";

/** The sections of a class page and a module page, in order. */
export const OUTLINE = [
  { id: "purpose", title: "Purpose" },
  { id: "members", title: "Members" },
  { id: "used-by", title: "Used by" },
  { id: "uses", title: "Uses" },
  { id: "source", title: "Source" },
];

/** The last part of a qualified name or a path. */
export function nameOf(id: string, isFile = false): string {
  const parts = id.split(isFile ? "/" : /::|\.|#/);
  return parts.at(-1) || id;
}

/** The qualified name without its last part; `null` when it has only one. */
export function ownerOf(id: string): string | null {
  const name = nameOf(id);
  const owner = id.slice(0, id.length - name.length).replace(/(::|\.|#)$/, "");
  return owner === "" || owner === id ? null : owner;
}

/** Each declaration once, at the deepest place it was found: a file answers for a member at every level. */
export function uniqueMembers(members: SymbolMember[]): SymbolMember[] {
  const rows = new Map<string, SymbolMember>();
  for (const member of members) {
    const key = `${member.id}:${member.lines[0]}`;
    const known = rows.get(key);
    if (known === undefined || member.depth > known.depth) rows.set(key, member);
  }
  return [...rows.values()];
}

const WORDS: Record<string, string> = { method: "Method", function: "Function", member: "Field", file: "File", module: "Module" };

/** What a person calls the symbol. The index stores `type`; the signature says which one. */
export function kindWord(kind: string, signature: string): string {
  if (kind === "type") {
    const word = /\b(class|interface|record|struct|enum|trait|mixin|extension|delegate)\b/.exec(signature)?.[1];
    return word === undefined ? "Type" : word.charAt(0).toUpperCase() + word.slice(1);
  }
  return WORDS[kind] ?? kind.charAt(0).toUpperCase() + kind.slice(1);
}

export function withArticle(word: string): string {
  const lower = word.toLowerCase();
  return `${/^[aeiou]/.test(lower) ? "An" : "A"} ${lower}`;
}

export function moduleOf(modules: Module[], path: string): Module | null {
  return modules.find((module) => module.files.includes(path)) ?? null;
}

/** A module by its folder's own name. */
export function moduleName(module: Module): string {
  return module.label.split("/").at(-1) || "(root)";
}

/** The last word of a folder path. */
export function folderName(path: string): string {
  return path.replace(/\/+$/, "").split("/").at(-1) || path;
}

/** Numbered lines of a source text; line ends stay out of the numbering. */
export function sourceLines(text: string, start: number): { number: number; text: string }[] {
  const lines = text.split(/\r?\n/);
  if (lines.at(-1) === "") lines.pop();
  return lines.map((line, at) => ({ number: start + at, text: line }));
}

/** A key symbol's snippet is `in <count> <signature>`. */
export function splitKeySymbol(snippet: string): { incoming: number | null; signature: string } {
  const match = /^in (\d+) (.*)$/s.exec(snippet);
  return match?.[1] === undefined ? { incoming: null, signature: snippet } : { incoming: Number(match[1]), signature: match[2] ?? "" };
}

/** `path#Symbol` as a target the class page takes: the symbol, or the path when there is none. */
export function entryTarget(target: string): string {
  return target.split("#")[1] || target;
}
