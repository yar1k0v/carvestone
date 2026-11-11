import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ApiConfig } from "@/api/types/ApiConfig";
import { fakeApi } from "@/test/fakeApi";
import { listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { ageOf } from "./model";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const CONFIG: ApiConfig = {
  task_prefix: "CS",
  statuses: { task: ["todo", "in-progress", "review", "done"] },
  priorities: ["high", "medium", "low"],
  task_kinds: ["feature"],
  fields: {},
};

const TODO = listRow("tasks/CS-1", { status: "todo", title: "First", priority: "low", etag: "e1" });
const LATER = listRow("tasks/CS-2", { status: "todo", title: "Second", priority: "high", etag: "e2" });
const TASKS_URL = "/api/items?type=task";

function board(extra: Record<string, { status?: number; body: unknown }> = {}) {
  return fakeApi({
    "/api/config": { body: CONFIG },
    "/api/items": { body: [listRow("milestones/M-2", { type: "milestone" })] },
    [TASKS_URL]: { body: [TODO, LATER] },
    ...extra,
  });
}

function column(status: string) {
  return within(screen.getByRole("region", { name: status }));
}

describe("board", () => {
  it("orders_cards_by_priority_then_number", async () => {
    board();
    renderApp("/board");
    await screen.findByText("First");
    const titles = column("To do")
      .getAllByRole("link")
      .map((link) => link.textContent);
    expect(titles).toEqual(["Second", "First"]);
  });

  it("cards_cannot_be_dragged", async () => {
    const { fetchMock } = board();
    renderApp("/board");
    const card = (await screen.findByText("First")).closest("[aria-label]");
    expect(card).not.toHaveAttribute("aria-roledescription");
    expect(card).not.toHaveAttribute("role", "button");
    expect(fetchMock.mock.calls.every(([, init]) => (init?.method ?? "GET") === "GET")).toBe(true);
  });

  it("milestone_filter_from_url", async () => {
    const { calls } = board({ [`${TASKS_URL}&milestone=M-2`]: { body: [LATER] } });
    renderApp("/board?milestone=M-2");
    await screen.findByText("Second");
    expect(column("To do").queryByText("First")).toBeNull();
    expect(calls).toContain(`${TASKS_URL}&milestone=M-2`);
    expect(screen.getByRole("combobox", { name: "Milestone" })).toHaveValue("M-2");
  });

  it("service_filter_from_url", async () => {
    const api = listRow("tasks/CS-2", { status: "todo", title: "Second", services: ["Api"], etag: "e2" });
    const web = listRow("tasks/CS-1", { status: "todo", title: "First", services: ["Web"], etag: "e1" });
    const { calls } = board({ [TASKS_URL]: { body: [web, api] } });
    renderApp("/board?service=Api");
    await screen.findByText("Second");
    expect(column("To do").queryByText("First")).toBeNull();
    expect(calls).toContain(TASKS_URL);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Service" })).toHaveValue("Api"));
    expect(within(screen.getByRole("combobox", { name: "Service" })).getAllByRole("option")).toHaveLength(3);
  });

  it("priority_high_filter_from_url", async () => {
    board();
    renderApp("/board?priority=high");
    await screen.findByText("Second");
    expect(column("To do").queryByText("First")).toBeNull();
    expect(screen.getByRole("button", { name: "High priority" })).toHaveAttribute("aria-pressed", "true");
  });

  it("a_card_shows_every_fact_and_a_bare_card_only_its_title", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-06T12:00:00Z"));
    const full = listRow("tasks/CS-3", {
      status: "in-progress",
      title: "Full",
      priority: "high",
      milestone: "M-2",
      services: ["Api", "Web"],
      created: "2026-10-30",
      criteria_done: 1,
      criteria_total: 3,
      assignee: "yaroslav",
    });
    const bare = listRow("tasks/CS-4", { status: "in-progress", title: "Bare" });
    board({
      "/api/items": { body: [listRow("milestones/M-2", { type: "milestone", title: "Launch" })] },
      [TASKS_URL]: { body: [full, bare] },
    });
    renderApp("/board");
    const card = (await screen.findByText("Full")).closest("[aria-label]") as HTMLElement;
    await within(card).findByText("Launch · Api · Web");
    expect(within(card).getByText("High")).toBeInTheDocument();
    expect(within(card).getByText("1/3")).toBeInTheDocument();
    expect(within(card).getByText("1 week")).toBeInTheDocument();
    expect(within(card).getByText("yaroslav")).toBeInTheDocument();

    const empty = screen.getByText("Bare").closest("[aria-label]") as HTMLElement;
    expect(empty.children).toHaveLength(2);
    expect(empty).toHaveTextContent(/^CS-4Bare$/);
  });

  it("a_click_on_the_done_count_adds_a_done_column", async () => {
    const done = listRow("tasks/CS-5", { status: "done", title: "Shipped", etag: "e5" });
    const user = userEvent.setup();
    board({ [TASKS_URL]: { body: [TODO, done] } });
    const router = renderApp("/board");
    await screen.findByText("First");
    expect(screen.queryByRole("region", { name: "Done" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Done 1" }));
    expect(await screen.findByRole("region", { name: "Done" })).toBeInTheDocument();
    expect(column("Done").getByText("Shipped")).toBeInTheDocument();
    expect(router.state.location.search).toMatchObject({ show: "done" });
    expect(screen.getByRole("button", { name: "Done 1" })).toHaveAttribute("aria-pressed", "true");
  });
});

describe("ageOf", () => {
  const today = new Date("2026-11-06T23:30:00Z");
  it.each([
    ["2026-11-06", "today"],
    ["2026-11-05", "1 day"],
    ["2026-10-31", "6 days"],
    ["2026-10-30", "1 week"],
    ["2026-10-24", "1 week"],
    ["2026-10-23", "2 weeks"],
    ["2026-09-08", "8 weeks"],
    ["2026-09-07", "2 months"],
    ["2026-08-08", "3 months"],
    ["2026-11-07", null],
    ["2026-13-45", null],
    ["2025-13-05", null],
    ["soon", null],
    [undefined, null],
  ])("%s is %s", (created, want) => {
    expect(ageOf(created, today)).toBe(want);
  });
});
