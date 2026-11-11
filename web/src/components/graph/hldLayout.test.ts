import { describe, expect, it } from "vitest";
import { CARD_W, layoutParts } from "./hldLayout";

describe("hld layout", () => {
  it("puts a caller left of what it calls", () => {
    const { boxes } = layoutParts(["db", "api", "web"], [
      { from: "web", to: "api" },
      { from: "api", to: "db" },
    ]);
    const x = (id: string) => boxes.find((box) => box.id === id)?.x ?? -1;
    expect(x("web")).toBeLessThan(x("api"));
    expect(x("api")).toBeLessThan(x("db"));
  });

  it("survives a cycle and keeps every part", () => {
    const { boxes } = layoutParts(["a", "b"], [
      { from: "a", to: "b" },
      { from: "b", to: "a" },
    ]);
    expect(boxes.map((box) => box.id).sort()).toEqual(["a", "b"]);
  });

  it("sets parts with no connection in a row below", () => {
    const { boxes, width } = layoutParts(["a", "b", "lone"], [{ from: "a", to: "b" }]);
    const lone = boxes.find((box) => box.id === "lone");
    const a = boxes.find((box) => box.id === "a");
    expect(lone?.y).toBeGreaterThan(a?.y ?? 0);
    expect(width).toBeGreaterThanOrEqual(CARD_W);
  });
});
