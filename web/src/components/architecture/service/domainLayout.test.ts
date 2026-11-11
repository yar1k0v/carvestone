import { describe, expect, it } from "vitest";
import type { EntityRelation } from "@/api/types/EntityRelation";
import { centredLayout } from "./domainLayout";

const relation = (from: string, to: string, label?: string): EntityRelation => (label === undefined ? { from, to } : { from, to, label });

describe("centred layout", () => {
  // Card 232 wide, 108 high, 88 between columns and 20 between rows, from `hldLayout`.
  const placed = (names: string[], relations: EntityRelation[], centre: string) => Object.fromEntries(centredLayout(names, relations, centre).boxes.map((box) => [box.id, [box.x, box.y]]));

  it("puts the centre in the middle column and splits the rest on both sides, related ones together", () => {
    const rows: [string, string[], EntityRelation[], Record<string, number[]>][] = [
      ["one neighbour sits right of the centre", ["Hub", "A"], [relation("Hub", "A")], { Hub: [0, 0], A: [320, 0] }],
      [
        "two unrelated neighbours go one to each side",
        ["Hub", "A", "B"],
        [relation("Hub", "A"), relation("B", "Hub")],
        { Hub: [320, 0], A: [640, 0], B: [0, 0] },
      ],
      [
        "a neighbour joins the side of the one it relates to, and a column is centred on the centre",
        ["Hub", "A", "B", "C"],
        [relation("Hub", "A"), relation("Hub", "B"), relation("Hub", "C"), relation("A", "C")],
        { Hub: [320, 64], A: [640, 0], B: [0, 64], C: [640, 128] },
      ],
    ];
    for (const [name, names, relations, expected] of rows) expect(placed(names, relations, "Hub"), name).toEqual(expected);
  });
});
