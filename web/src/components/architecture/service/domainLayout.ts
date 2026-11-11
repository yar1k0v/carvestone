import type { Entity } from "@/api/types/Entity";
import type { EntityField } from "@/api/types/EntityField";
import type { EntityRelation } from "@/api/types/EntityRelation";
import { CARD_H, CARD_W, type Layout } from "@/components/graph/hldLayout";

/** The spaces `hldLayout` leaves between cards, so the two drawings look alike. */
const GAP_X = 88;
const GAP_Y = 20;

/** One field as the cards and the panel write it: `Id: Guid`, or just the name when no type is known. */
export function fieldText(field: EntityField): string {
  return field.type === undefined ? field.name : `${field.name}: ${field.type}`;
}

/** What an entity points at, one line each: `has many Line`. */
export function relationLines(entity: Entity, relations: EntityRelation[]): string[] {
  return relations.filter((relation) => relation.from === entity.name).map((relation) => `${relation.label ?? "relates to"} ${relation.to}`);
}

/**
 * The centre in the middle column and the entities around it in the columns on either side.
 * An entity joins the side that already holds more of the entities it relates to, so lines between
 * neighbours stay on one side of the centre.
 */
export function centredLayout(names: string[], relations: EntityRelation[], centre: string): Layout {
  const left: string[] = [];
  const right: string[] = [];
  const near = (name: string, side: string[]) =>
    relations.filter((relation) => (relation.from === name && side.includes(relation.to)) || (relation.to === name && side.includes(relation.from))).length;
  for (const name of names.filter((each) => each !== centre)) {
    const toLeft = near(name, left);
    const toRight = near(name, right);
    const goesLeft = toLeft !== toRight ? toLeft > toRight : left.length < right.length;
    (goesLeft ? left : right).push(name);
  }
  const columnHeight = (count: number) => (count === 0 ? 0 : count * CARD_H + (count - 1) * GAP_Y);
  const height = Math.max(CARD_H, columnHeight(left.length), columnHeight(right.length));
  const step = CARD_W + GAP_X;
  const middle = left.length === 0 ? 0 : step;
  const boxes: Layout["boxes"] = [{ id: centre, x: middle, y: (height - CARD_H) / 2 }];
  const stack = (ids: string[], x: number) => {
    const top = (height - columnHeight(ids.length)) / 2;
    ids.forEach((id, at) => boxes.push({ id, x, y: top + at * (CARD_H + GAP_Y) }));
  };
  stack(left, 0);
  stack(right, middle + step);
  const width = boxes.reduce((most, box) => Math.max(most, box.x + CARD_W), 0);
  return { boxes, width, height, aloneTop: null };
}
