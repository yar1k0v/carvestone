/** The C4 level a diagram draws, from its first line. */
export type C4Level = "C4Context" | "C4Container" | "C4Component" | "C4Dynamic" | "C4Deployment";

export type C4Kind = "person" | "system" | "container" | "component" | "node" | "boundary";

export type C4Shape = "box" | "db" | "queue";

export interface C4Element {
  alias: string;
  kind: C4Kind;
  shape: C4Shape;
  external: boolean;
  label: string;
  technology?: string;
  description?: string;
  /** A stone item id from `$link`. */
  link?: string;
  /** The alias of the boundary or element it sits in. */
  parent?: string;
  /** What a boundary or deployment node says it is, such as "system" or "container". */
  type?: string;
}

export interface C4Rel {
  from: string;
  to: string;
  label: string;
  technology?: string;
  description?: string;
  bidirectional: boolean;
  /** The step number of a `C4Dynamic` relationship. */
  index?: string;
}

export interface C4Diagram {
  level: C4Level;
  title?: string;
  elements: C4Element[];
  rels: C4Rel[];
  /** Lines that were not understood, with their line number in the source (1-based). */
  skipped: { line: number; text: string }[];
}

export type Parsed = { diagram: C4Diagram } | { error: string };

const LEVELS = new Set<string>(["C4Context", "C4Container", "C4Component", "C4Dynamic", "C4Deployment"]);

/** Kind words as a C4 reader says them: "External Software System", "Container (database)". */
export function kindLabel(element: C4Element): string {
  const base =
    element.kind === "person"
      ? "Person"
      : element.kind === "system"
        ? "Software System"
        : element.kind === "container"
          ? "Container"
          : element.kind === "component"
            ? "Component"
            : element.kind === "node"
              ? "Deployment Node"
              : "Boundary";
  const shape = element.shape === "db" ? " (database)" : element.shape === "queue" ? " (queue)" : "";
  return `${element.external ? "External " : ""}${base}${shape}`;
}

/** The leading words of a diagram's first line that mark it as C4. */
export function isC4(source: string): boolean {
  const first = source.split("\n").find((line) => line.trim() !== "");
  return first !== undefined && LEVELS.has(first.trim().split(/\s/)[0] ?? "");
}

interface Macro {
  kind: C4Kind;
  shape: C4Shape;
  external: boolean;
  /** Where the technology sits in the positional arguments after alias and label, if anywhere. */
  technology: number | null;
  description: number | null;
  type: number | null;
}

function element(kind: C4Kind, technology: number | null, description: number | null, type: number | null = null) {
  const out: Record<string, Macro> = {};
  const base = kind === "person" ? "Person" : kind[0]?.toUpperCase() + kind.slice(1);
  const shapes: [string, C4Shape][] = kind === "person" ? [["", "box"]] : [["", "box"], ["Db", "db"], ["Queue", "queue"]];
  for (const [suffix, shape] of shapes) {
    out[`${base}${suffix}`] = { kind, shape, external: false, technology, description, type };
    out[`${base}${suffix}_Ext`] = { kind, shape, external: true, technology, description, type };
  }
  return out;
}

const ELEMENTS: Record<string, Macro> = {
  ...element("person", null, 0),
  ...element("system", null, 0),
  ...element("container", 0, 1),
  ...element("component", 0, 1),
};

const BOUNDARIES: Record<string, { kind: C4Kind; type: string | null }> = {
  Boundary: { kind: "boundary", type: null },
  Enterprise_Boundary: { kind: "boundary", type: "enterprise" },
  System_Boundary: { kind: "boundary", type: "system" },
  Container_Boundary: { kind: "boundary", type: "container" },
  Deployment_Node: { kind: "node", type: null },
  Node: { kind: "node", type: null },
  Node_L: { kind: "node", type: null },
  Node_R: { kind: "node", type: null },
};

const RELS = new Set([
  "Rel",
  "BiRel",
  "Rel_Back",
  "Rel_Up",
  "Rel_U",
  "Rel_Down",
  "Rel_D",
  "Rel_Left",
  "Rel_L",
  "Rel_Right",
  "Rel_R",
  "RelIndex",
]);

/** Statements that only style or place things; the viewer lays out on its own. */
const IGNORED = /^(Update\w*|Add\w*Tag|LAYOUT_\w*|SHOW_\w*|HIDE_\w*)$/;

interface Call {
  name: string;
  positional: string[];
  named: Record<string, string>;
  opens: boolean;
}

/** Strips one pair of quotes and reads Mermaid's `#quot;` as the double quote it stands for. */
function unquote(value: string): string {
  const trimmed = value.trim();
  const inner = trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"') ? trimmed.slice(1, -1) : trimmed;
  return inner.replaceAll("#quot;", '"');
}

/** `Name(a, "b, c", $link="x") {` split into its parts; null when the line is no call. */
function call(line: string): Call | null {
  const match = /^([A-Za-z_][\w]*)\s*\((.*)\)\s*(\{)?\s*$/.exec(line);
  if (match === null) return null;
  const [, name = "", inner = "", brace] = match;
  const args: string[] = [];
  let current = "";
  let quoted = false;
  for (const char of inner) {
    if (char === '"') quoted = !quoted;
    if (char === "," && !quoted) {
      args.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  if (current.trim() !== "" || args.length > 0) args.push(current);
  const positional: string[] = [];
  const named: Record<string, string> = {};
  for (const arg of args) {
    const pair = /^\s*\$(\w+)\s*=\s*(.*)$/.exec(arg);
    if (pair) named[pair[1] ?? ""] = unquote(pair[2] ?? "");
    else positional.push(unquote(arg));
  }
  return { name, positional, named, opens: brace !== undefined };
}

function text(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === "" ? undefined : trimmed;
}

/** Reads a Mermaid C4 diagram: elements, boundaries and relationships. Styling statements are ignored. */
export function parseC4(source: string): Parsed {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() !== "");
  const level = lines[start]?.trim().split(/\s/)[0] ?? "";
  if (!LEVELS.has(level)) return { error: "A C4 diagram starts with C4Context, C4Container, C4Component, C4Dynamic or C4Deployment." };
  const diagram: C4Diagram = { level: level as C4Level, elements: [], rels: [], skipped: [] };
  const stack: string[] = [];
  const aliases = new Set<string>();
  const rels: { rel: C4Rel; line: number; text: string }[] = [];
  for (let i = start + 1; i < lines.length; i++) {
    const line = (lines[i] ?? "").trim();
    if (line === "" || line.startsWith("%%")) continue;
    if (line === "}") {
      stack.pop();
      continue;
    }
    if (line === "{") continue;
    const title = /^title\s+(.*)$/.exec(line);
    if (title) {
      diagram.title = title[1]?.trim();
      continue;
    }
    const parsed = call(line);
    if (parsed === null) {
      diagram.skipped.push({ line: i + 1, text: line });
      continue;
    }
    const { name, positional, named } = parsed;
    const parent = stack.findLast((alias) => alias !== "");
    const elementMacro = ELEMENTS[name];
    const boundary = BOUNDARIES[name];
    if (elementMacro !== undefined || boundary !== undefined) {
      const [alias = "", label = ""] = positional;
      const rest = positional.slice(2);
      if (alias === "" || aliases.has(alias)) {
        diagram.skipped.push({ line: i + 1, text: line });
        if (parsed.opens) stack.push(alias);
        continue;
      }
      aliases.add(alias);
      const pick = (at: number | null | undefined) => (at === null || at === undefined ? undefined : text(rest[at]));
      const found: C4Element =
        elementMacro !== undefined
          ? {
              alias,
              kind: elementMacro.kind,
              shape: elementMacro.shape,
              external: elementMacro.external,
              label: label || alias,
            }
          : { alias, kind: boundary?.kind ?? "boundary", shape: "box", external: false, label: label || alias };
      const technology = text(named.techn) ?? pick(elementMacro?.technology);
      const description =
        text(named.descr) ?? (elementMacro !== undefined ? pick(elementMacro.description) : boundary?.kind === "node" ? text(rest[1]) : undefined);
      const type = boundary?.type ?? text(rest[0]);
      if (technology !== undefined) found.technology = technology;
      if (description !== undefined) found.description = description;
      if (text(named.link) !== undefined) found.link = text(named.link);
      if (parent !== undefined) found.parent = parent;
      if (boundary !== undefined && type !== undefined) found.type = type;
      diagram.elements.push(found);
      if (parsed.opens) stack.push(alias);
      continue;
    }
    if (RELS.has(name)) {
      const args = name === "RelIndex" ? positional.slice(1) : positional;
      const [first = "", second = "", label = "", technology, description] = args;
      const back = name === "Rel_Back";
      const rel: C4Rel = {
        from: back ? second : first,
        to: back ? first : second,
        label,
        bidirectional: name === "BiRel",
      };
      const tech = text(named.techn) ?? text(technology);
      const descr = text(named.descr) ?? text(description);
      if (tech !== undefined) rel.technology = tech;
      if (descr !== undefined) rel.description = descr;
      if (name === "RelIndex" && text(positional[0]) !== undefined) rel.index = text(positional[0]);
      rels.push({ rel, line: i + 1, text: line });
      continue;
    }
    if (IGNORED.test(name)) continue;
    diagram.skipped.push({ line: i + 1, text: line });
    if (parsed.opens) stack.push("");
  }
  // A relationship to an alias the diagram never declares cannot be drawn.
  for (const { rel, line, text: written } of rels) {
    if (aliases.has(rel.from) && aliases.has(rel.to)) diagram.rels.push(rel);
    else diagram.skipped.push({ line, text: written });
  }
  diagram.skipped.sort((a, b) => a.line - b.line);
  // C4Dynamic numbers its steps in order when they carry no index.
  if (diagram.level === "C4Dynamic") diagram.rels.forEach((rel, at) => (rel.index ??= String(at + 1)));
  return { diagram };
}
