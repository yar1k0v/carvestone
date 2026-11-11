/** A connection between two parts, by id. */
export interface LayoutEdge {
  from: string;
  to: string;
}

export interface Box {
  id: string;
  x: number;
  y: number;
}

export interface Layout {
  boxes: Box[];
  width: number;
  height: number;
  /** Where the row of parts with no connection starts, when there is a drawing above it. */
  aloneTop: number | null;
}

export const CARD_W = 232;
export const CARD_H = 108;
const GAP_X = 88;
const GAP_Y = 20;

/** The rank of each id: the longest way in from a part nothing points at. A cycle stops counting at its first repeat. */
function ranks(ids: string[], edges: LayoutEdge[]): Map<string, number> {
  const into = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const edge of edges) into.get(edge.to)?.push(edge.from);
  const rank = new Map<string, number>();
  const visit = (id: string, path: Set<string>): number => {
    const known = rank.get(id);
    if (known !== undefined) return known;
    path.add(id);
    let best = 0;
    for (const from of into.get(id) ?? []) {
      if (!path.has(from)) best = Math.max(best, visit(from, path) + 1);
    }
    path.delete(id);
    rank.set(id, best);
    return best;
  };
  for (const id of ids) visit(id, new Set());
  return rank;
}

/**
 * Columns left to right by who calls whom, each column ordered near the parts it connects to.
 * Parts with no connection sit in rows below. Plain arithmetic, so the first paint needs no layout library.
 */
export function layoutParts(ids: string[], allEdges: LayoutEdge[]): Layout {
  const known = new Set(ids);
  const edges = allEdges.filter((edge) => edge.from !== edge.to && known.has(edge.from) && known.has(edge.to));
  const linked = new Set(edges.flatMap((edge) => [edge.from, edge.to]));
  const connected = ids.filter((id) => linked.has(id));
  const alone = ids.filter((id) => !linked.has(id));

  const rank = ranks(connected, edges);
  const columns: string[][] = [];
  for (const id of connected) (columns[rank.get(id) ?? 0] ??= []).push(id);
  const filled = columns.filter((column) => column !== undefined);

  // Two sweeps of the barycentre rule keep neighbours level, which uncrosses most lines.
  const row = new Map<string, number>();
  const place = () => filled.forEach((column) => column.forEach((id, at) => row.set(id, at)));
  place();
  const neighbours = (id: string) => edges.flatMap((edge) => (edge.from === id ? [edge.to] : edge.to === id ? [edge.from] : []));
  const centre = (id: string) => {
    const near = neighbours(id).map((other) => row.get(other) ?? 0);
    return near.length === 0 ? (row.get(id) ?? 0) : near.reduce((sum, at) => sum + at, 0) / near.length;
  };
  for (let sweep = 0; sweep < 2; sweep += 1) {
    for (const column of filled) {
      const weight = new Map(column.map((id) => [id, centre(id)]));
      column.sort((a, b) => (weight.get(a) ?? 0) - (weight.get(b) ?? 0));
    }
    place();
  }

  const tallest = filled.reduce((most, column) => Math.max(most, column.length), 0);
  const graphH = tallest === 0 ? 0 : tallest * CARD_H + (tallest - 1) * GAP_Y;
  const boxes: Box[] = [];
  filled.forEach((column, at) => {
    const columnH = column.length * CARD_H + (column.length - 1) * GAP_Y;
    const top = (graphH - columnH) / 2;
    column.forEach((id, index) => boxes.push({ id, x: at * (CARD_W + GAP_X), y: top + index * (CARD_H + GAP_Y) }));
  });

  const perRow = Math.max(filled.length, 3);
  const aloneTop = graphH === 0 ? 0 : graphH + GAP_Y * 3;
  alone.forEach((id, at) => {
    boxes.push({ id, x: (at % perRow) * (CARD_W + GAP_Y), y: aloneTop + Math.floor(at / perRow) * (CARD_H + GAP_Y) });
  });

  const width = boxes.reduce((most, box) => Math.max(most, box.x + CARD_W), 0);
  const height = boxes.reduce((most, box) => Math.max(most, box.y + CARD_H), 0);
  return { boxes, width, height, aloneTop: graphH === 0 || alone.length === 0 ? null : aloneTop };
}

/** A smooth line out of the side of one card into the facing side of the other. */
export function edgePath(from: Box, to: Box): string {
  const forward = to.x > from.x;
  const backward = to.x < from.x;
  const sx = from.x + (backward ? 0 : CARD_W);
  const tx = to.x + (forward ? 0 : CARD_W);
  const sy = from.y + CARD_H / 2;
  const ty = to.y + CARD_H / 2;
  const bend = forward || backward ? Math.max(40, Math.abs(tx - sx) / 2) : 56;
  const out = backward ? -bend : bend;
  const back = forward ? -bend : bend;
  return `M ${sx} ${sy} C ${sx + out} ${sy}, ${tx + back} ${ty}, ${tx} ${ty}`;
}
