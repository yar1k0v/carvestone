import type { DesignDoc } from "@/api/types/DesignDoc";

/** `Mobile flow: vehicle list to live map (LLD)` reads `Vehicle list to live map`. */
export function lldTitle(title: string): string {
  const plain = title
    .replace(/\s*\([^()]*\bLLD\b[^()]*\)\s*$/i, "")
    .replace(/^[\w -]*\bflow:\s*/i, "")
    .trim();
  const shown = plain === "" ? title : plain;
  return `${shown.slice(0, 1).toUpperCase()}${shown.slice(1)}`;
}

export interface LldParts {
  flows: DesignDoc[];
  components: DesignDoc[];
  reference: DesignDoc[];
}

/** The docs of a container by what they describe, each by title. */
export function lldParts(docs: DesignDoc[]): LldParts {
  const sorted = [...docs].sort((a, b) => lldTitle(a.title).localeCompare(lldTitle(b.title)));
  return {
    flows: sorted.filter((doc) => doc.kind === "flow"),
    components: sorted.filter((doc) => doc.kind === "component"),
    reference: sorted.filter((doc) => doc.kind !== "flow" && doc.kind !== "component"),
  };
}
