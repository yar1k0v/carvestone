import { describe, expect, it } from "vitest";
import { slugs } from "./slug";

describe("anchors", () => {
  // The cases of `slug_rules` in crates/core/src/section.rs; both sides must agree on every one.
  it("anchors_match_engine", () => {
    for (const [heading, want] of [
      ["Acceptance criteria", "acceptance-criteria"],
      ["Data model", "data-model"],
      ["Why? (really)", "why-really"],
      ["A & B", "a--b"],
      ["snake_case Name-1", "snake_case-name-1"],
      ["  Padded  ", "padded"],
      ["Äpfel", "äpfel"],
    ] as const) {
      expect(slugs([heading]), heading).toEqual([want]);
    }
  });

  // `duplicate_anchors_get_a_suffix` in the engine, and AC-7.1.
  it("repeated_headings_get_a_suffix", () => {
    expect(slugs(["B", "B"])).toEqual(["b", "b-1"]);
    expect(slugs(["Notes", "Notes", "Notes"])).toEqual(["notes", "notes-1", "notes-2"]);
  });
});
