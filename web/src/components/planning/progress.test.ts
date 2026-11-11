import { describe, expect, it } from "vitest";
import { listRow } from "@/test/fixtures";
import { type MilestoneState, milestoneState } from "./progress";

/** Tasks as (ready, running, review, done) counts, each with the status a store writes. */
function tasks(ready: number, running: number, review: number, done: number) {
  const statuses = [
    ...Array<string>(ready).fill("todo"),
    ...Array<string>(running).fill("in-progress"),
    ...Array<string>(review).fill("review"),
    ...Array<string>(done).fill("done"),
  ];
  return statuses.map((status, at) => listRow(`tasks/CS-${at + 1}`, { status }));
}

describe("milestone state", () => {
  // The first three rows are the Milestones board; the rest cover the empty and finished ends.
  const cases: [string, ReturnType<typeof tasks>, MilestoneState][] = [
    ["support area redesign", tasks(0, 1, 1, 39), "Closing"],
    ["stability and scaling", tasks(19, 3, 1, 6), "Running"],
    ["backlog triage", tasks(7, 0, 0, 15), "Paused"],
    ["only ready tasks", tasks(3, 0, 0, 0), "Planned"],
    ["no tasks", [], "Planned"],
    ["everything done", tasks(0, 0, 0, 4), "Done"],
  ];

  it.each(cases)("%s", (_name, rows, state) => {
    expect(milestoneState(rows)).toBe(state);
  });
});
