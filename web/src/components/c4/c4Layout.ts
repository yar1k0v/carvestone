import { type Edge, MarkerType, type Node } from "@xyflow/react";
import type { ELK, ElkExtendedEdge, ElkNode, ElkPoint } from "elkjs/lib/elk-api";
import { type C4Diagram, type C4Element, type C4Rel, kindLabel } from "./parse";

export interface C4NodeData extends Record<string, unknown> {
  element: C4Element;
  /** Elements drawn inside it; 0 for a leaf. */
  inside: number;
  /** Something else is selected and this element is not one of its ends. */
  faded: boolean;
  /** Clicking through opens something, so the box shows the link icon. */
  opens: boolean;
}

export type C4FlowNode = Node<C4NodeData, "c4" | "c4Group" | "c4Name">;

export interface C4EdgeData extends Record<string, unknown> {
  rel: C4Rel;
  /** The route on the whole drawing: start, bends, end. */
  points: ElkPoint[];
  /** The lines of the label, as drawn. */
  lines: string[];
  /** Where the label sits: the place the layout kept free for it, else the middle of the route. */
  labelBox: Box;
}

export type C4FlowEdge = Edge<C4EdgeData, "c4Edge">;

export const LEAF_WIDTH = 240;
/** Room above a boundary's children for its name, and around them. */
const GROUP_PADDING = "[top=40,left=32,bottom=32,right=32]";

/** How many characters of each size fit on a line of a leaf, from the font widths the boxes use. */
const NAME_CHARS = 28;
const TAG_CHARS = 36;
const TEXT_CHARS = 33;

function linesFor(text: string | undefined, perLine: number): number {
  return text === undefined || text === "" ? 0 : Math.ceil(text.length / perLine);
}

/** The type words in brackets under a box's name: `[Person]`, `[Container: Rust]`, `[Software System, outside]`. */
export function typeTag(element: C4Element): string {
  const base = kindLabel({ ...element, external: false });
  const outside = element.external ? ", outside" : "";
  const technology = element.technology === undefined ? "" : `: ${element.technology}`;
  return `[${base}${technology}${outside}]`;
}

/** A leaf is as tall as its name, type tag and description need. */
export function leafHeight(element: C4Element): number {
  const text = linesFor(element.label, NAME_CHARS) * 18 + linesFor(typeTag(element), TAG_CHARS) * 15 + linesFor(element.description, TEXT_CHARS) * 16;
  return Math.max(64, text + (element.description === undefined ? 0 : 4) + 20);
}

/** Average width of one character of the 11px label text. */
const LABEL_CHAR = 6.2;
const LABEL_CHARS = 26;

function wrap(text: string, perLine: number): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/).filter((w) => w !== "")) {
    if (line !== "" && line.length + 1 + word.length > perLine) {
      lines.push(line);
      line = word;
    } else {
      line = line === "" ? word : `${line} ${word}`;
    }
  }
  return line === "" ? lines : [...lines, line];
}

/** The lines an arrow's label is drawn in: what it does, then its technology in brackets. */
export function labelLines(rel: C4Rel): string[] {
  const step = rel.index === undefined ? "" : `${rel.index}. `;
  return [...wrap(`${step}${rel.label}`, LABEL_CHARS), ...(rel.technology === undefined ? [] : wrap(`[${rel.technology}]`, LABEL_CHARS))];
}

/** The size the layout keeps free for a label. */
export function labelSize(lines: string[]): { width: number; height: number } {
  return { width: Math.ceil(Math.max(0, ...lines.map((line) => line.length)) * LABEL_CHAR) + 14, height: lines.length * 14 + 6 };
}

/** What an arrow says: its step, label and technology. */
export function relText(rel: C4Rel): string {
  const step = rel.index === undefined ? "" : `${rel.index}. `;
  const technology = rel.technology === undefined ? "" : ` [${rel.technology}]`;
  return `${step}${rel.label}${technology}`;
}

export const REL_COLOR = "#71717a";
export const ACTIVE_COLOR = "var(--primary)";

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Which arrows carry their label: all of them, or only those of the selected element. */
export type LabelMode = "all" | "selected";

/** Past this many arrows their labels cover each other, so they show for the selection only. */
export const LABELS_ALL_UP_TO = 8;

/** The size of a boundary's name and type, as the 13px semibold text takes it. */
export function nameSize(element: C4Element): { width: number; height: number } {
  return { width: Math.ceil(`${element.label} [${element.type ?? kindLabel(element)}]`.length * 7.4) + 10, height: 20 };
}

/**
 * Where inside a boundary its name goes, relative to the boundary: the first of the top left, top
 * right, bottom left and bottom right corners that no arrow or label crosses. The boundary leaves
 * room for the name above and below its boxes. With every corner crossed it stays top left.
 */
export function nameCorner(element: C4Element, boundary: Box, crossed: (area: Box) => boolean): { x: number; y: number } {
  const size = nameSize(element);
  const margin = 8;
  const corners = [
    { x: margin, y: margin },
    { x: boundary.width - size.width - margin, y: margin },
    { x: margin, y: boundary.height - size.height - margin },
    { x: boundary.width - size.width - margin, y: boundary.height - size.height - margin },
  ];
  const free = corners.find((corner) => !crossed({ x: boundary.x + corner.x - 2, y: boundary.y + corner.y - 2, width: size.width + 4, height: size.height + 4 }));
  return free ?? (corners[0] as { x: number; y: number });
}

/** Where everything sits: boxes relative to the box they are in, routes on the whole drawing. */
export interface C4Geometry {
  boxes: Map<string, Box>;
  routes: Map<number, ElkPoint[]>;
  /** The place kept free for each arrow's label, on the whole drawing; empty when labels were not reserved. */
  labels: Map<number, Box>;
  /** The size of the whole drawing. */
  width: number;
  height: number;
}

export interface C4Layout {
  nodes: C4FlowNode[];
  edges: C4FlowEdge[];
}

const ROOT_OPTIONS = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  // One layout across the boundaries, so an arrow into a boundary is routed with everything else.
  "elk.hierarchyHandling": "INCLUDE_CHILDREN",
  "elk.edgeRouting": "ORTHOGONAL",
  "elk.layered.considerModelOrder.strategy": "NODES_AND_EDGES",
  "elk.spacing.nodeNode": "48",
  "elk.layered.spacing.nodeNodeBetweenLayers": "28",
  "elk.spacing.edgeNode": "24",
  "elk.layered.spacing.edgeNodeBetweenLayers": "20",
  "elk.spacing.edgeEdge": "16",
  "elk.layered.spacing.edgeEdgeBetweenLayers": "16",
  "elk.spacing.edgeLabel": "8",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.json.edgeCoords": "ROOT",
};

let elk: Promise<ELK> | null = null;

/** ELK is large, so it loads with the first diagram rather than with the app. */
function loadElk(): Promise<ELK> {
  elk ??= import("elkjs/lib/elk.bundled.js").then(({ default: Elk }) => new Elk());
  return elk;
}

/**
 * Places a C4 diagram top to bottom with ELK: boundaries wrap their children, and every arrow is a
 * right-angled route that goes around the boxes instead of across them. With `reserveLabels` each
 * label gets its real size in the layout, so it sits clear of boxes, other lines and other labels.
 */
export async function placeC4(diagram: C4Diagram, reserveLabels = false): Promise<C4Geometry> {
  const byAlias = new Map(diagram.elements.map((element) => [element.alias, element]));
  const parentOf = (alias: string): string | undefined => {
    const parent = byAlias.get(alias)?.parent;
    return parent !== undefined && byAlias.has(parent) ? parent : undefined;
  };
  const childrenOf = new Map<string | undefined, string[]>();
  for (const element of diagram.elements) {
    const parent = parentOf(element.alias);
    childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), element.alias]);
  }
  const node = (alias: string): ElkNode => {
    const kids = childrenOf.get(alias);
    if (kids !== undefined) return { id: alias, children: kids.map(node), layoutOptions: { "elk.padding": GROUP_PADDING } };
    return { id: alias, width: LEAF_WIDTH, height: leafHeight(byAlias.get(alias) as C4Element) };
  };
  const inside = (alias: string, outer: string): boolean => {
    for (let at = parentOf(alias); at !== undefined; at = parentOf(at)) if (at === outer) return true;
    return false;
  };
  const edges: ElkExtendedEdge[] = [];
  diagram.rels.forEach((rel, at) => {
    // An arrow from a box to the boundary around it has no route outside that boundary.
    if (rel.from === rel.to || inside(rel.from, rel.to) || inside(rel.to, rel.from)) return;
    const lines = reserveLabels ? labelLines(rel) : [];
    edges.push({
      id: `r${at}`,
      sources: [rel.from],
      targets: [rel.to],
      ...(lines.length === 0 ? {} : { labels: [{ text: lines.join(" "), ...labelSize(lines) }] }),
    });
  });
  const graph = await (await loadElk()).layout({
    id: "root",
    layoutOptions: ROOT_OPTIONS,
    children: (childrenOf.get(undefined) ?? []).map(node),
    edges,
  });

  const boxes = new Map<string, Box>();
  const visit = (placed: ElkNode) => {
    for (const child of placed.children ?? []) {
      boxes.set(child.id, { x: child.x ?? 0, y: child.y ?? 0, width: child.width ?? 0, height: child.height ?? 0 });
      visit(child);
    }
  };
  visit(graph);
  const routes = new Map<number, ElkPoint[]>();
  const labels = new Map<number, Box>();
  for (const edge of (graph.edges ?? []) as ElkExtendedEdge[]) {
    const points = (edge.sections ?? []).flatMap((s) => [s.startPoint, ...(s.bendPoints ?? []), s.endPoint]);
    if (points.length >= 2) routes.set(Number(edge.id.slice(1)), points);
    const label = edge.labels?.[0];
    if (label?.x !== undefined && label.y !== undefined) {
      labels.set(Number(edge.id.slice(1)), { x: label.x, y: label.y, width: label.width ?? 0, height: label.height ?? 0 });
    }
  }
  const whole = graph as ElkNode;
  return { boxes, routes, labels, width: whole.width ?? 0, height: whole.height ?? 0 };
}

/** The point halfway along a route, where its label sits. */
export function halfway(points: ElkPoint[]): ElkPoint {
  const lengths = points.slice(1).map((p, i) => Math.abs(p.x - (points[i] as ElkPoint).x) + Math.abs(p.y - (points[i] as ElkPoint).y));
  let left = lengths.reduce((a, b) => a + b, 0) / 2;
  for (const [i, length] of lengths.entries()) {
    const from = points[i] as ElkPoint;
    const to = points[i + 1] as ElkPoint;
    if (left <= length && length > 0) {
      const t = left / length;
      return { x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t };
    }
    left -= length;
  }
  return points[0] ?? { x: 0, y: 0 };
}

/**
 * The React Flow nodes and edges for a placed diagram. With `selected`, the arrows of that element
 * stand out and every other element fades; `labels` says which arrows carry their text.
 */
export function drawC4(
  diagram: C4Diagram,
  geometry: C4Geometry,
  selected: string | null = null,
  labels: LabelMode = "all",
  opens: (element: C4Element) => boolean = (element) => element.link !== undefined,
): C4Layout {
  const byAlias = new Map(diagram.elements.map((element) => [element.alias, element]));
  const parentOf = (element: C4Element) =>
    element.parent !== undefined && byAlias.has(element.parent) ? element.parent : undefined;
  const depth = (element: C4Element): number => {
    let n = 0;
    for (let at = parentOf(element); at !== undefined; at = parentOf(byAlias.get(at) as C4Element)) n += 1;
    return n;
  };
  const inside = new Map<string, number>();
  for (const element of diagram.elements) {
    const parent = parentOf(element);
    if (parent !== undefined) inside.set(parent, (inside.get(parent) ?? 0) + 1);
  }
  const near = new Set<string>();
  if (selected !== null) {
    near.add(selected);
    for (const rel of diagram.rels) {
      if (rel.from === selected) near.add(rel.to);
      if (rel.to === selected) near.add(rel.from);
    }
  }
  // React Flow wants a parent before its children.
  const ordered = [...diagram.elements].sort((a, b) => depth(a) - depth(b));
  const nodes: C4FlowNode[] = ordered.flatMap((element) => {
    const box = geometry.boxes.get(element.alias);
    if (box === undefined) return [];
    const parent = parentOf(element);
    const group = inside.has(element.alias);
    return [
      {
        id: element.alias,
        type: group ? "c4Group" : "c4",
        position: { x: box.x, y: box.y },
        width: box.width,
        height: box.height,
        ...(parent === undefined ? {} : { parentId: parent }),
        data: {
          element,
          inside: inside.get(element.alias) ?? 0,
          faded: selected !== null && !group && !near.has(element.alias),
          opens: opens(element),
        },
      },
    ];
  });

  // A boundary's name is a node of its own, set in the corner of the boundary that no arrow or label crosses.
  const absolute = (alias: string): Box => {
    const box = geometry.boxes.get(alias) as Box;
    const parent = (byAlias.get(alias) as C4Element).parent;
    const outer = parent !== undefined && geometry.boxes.has(parent) ? absolute(parent) : { x: 0, y: 0 };
    return { ...box, x: box.x + outer.x, y: box.y + outer.y };
  };
  const crossed = (area: Box): boolean => {
    const meets = (b: Box) => area.x < b.x + b.width && b.x < area.x + area.width && area.y < b.y + b.height && b.y < area.y + area.height;
    for (const points of geometry.routes.values()) {
      for (const [i, q] of points.slice(1).entries()) {
        const p = points[i] as ElkPoint;
        if (meets({ x: Math.min(p.x, q.x), y: Math.min(p.y, q.y), width: Math.abs(p.x - q.x), height: Math.abs(p.y - q.y) })) return true;
      }
    }
    return [...geometry.labels.values()].some(meets);
  };
  for (const node of [...nodes]) {
    if (node.type !== "c4Group") continue;
    const place = nameCorner(node.data.element, absolute(node.id), crossed);
    nodes.push({
      id: `${node.id}#name`,
      type: "c4Name",
      position: place,
      parentId: node.id,
      zIndex: 2000,
      selectable: false,
      draggable: false,
      focusable: false,
      className: "pointer-events-none",
      data: { ...node.data, faded: false, opens: false },
    });
  }

  const edges: C4FlowEdge[] = diagram.rels.flatMap((rel, at) => {
    const points = geometry.routes.get(at);
    if (points === undefined) return [];
    const active = selected !== null && (rel.from === selected || rel.to === selected);
    const faded = selected !== null && !active;
    const color = active ? ACTIVE_COLOR : REL_COLOR;
    const label = labels === "all" || active ? relText(rel) : undefined;
    const lines = labelLines(rel);
    const size = labelSize(lines);
    const middle = halfway(points);
    const labelBox = geometry.labels.get(at) ?? { x: middle.x - size.width / 2, y: middle.y - size.height / 2, ...size };
    const marker = { type: MarkerType.ArrowClosed, color, width: 14, height: 14 };
    return [
      {
        id: `r${at}`,
        type: "c4Edge" as const,
        source: rel.from,
        target: rel.to,
        ...(label === undefined ? {} : { label }),
        // Drawn above the boxes so a line is never hidden, but clicks go through to the box below.
        zIndex: 1000,
        className: "pointer-events-none",
        interactionWidth: 0,
        selectable: false,
        focusable: false,
        style: { stroke: color, strokeWidth: active ? 2 : 1, opacity: active ? 1 : faded ? 0.15 : 0.8 },
        markerEnd: marker,
        ...(rel.bidirectional ? { markerStart: marker } : {}),
        labelStyle: { fill: active ? ACTIVE_COLOR : "currentColor", fontSize: 11, opacity: faded ? 0.25 : 1 },
        data: { rel, points, lines, labelBox },
      },
    ];
  });
  return { nodes, edges };
}

/** Place and draw in one step. */
export async function layoutC4(diagram: C4Diagram, selected: string | null = null, labels: LabelMode = "all"): Promise<C4Layout> {
  return drawC4(diagram, await placeC4(diagram, labels === "all"), selected, labels);
}
