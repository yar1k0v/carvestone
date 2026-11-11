import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "@/App";
import type { Backlink } from "@/api/types/Backlink";
import { createMemoryRouter, createQueryClient } from "@/router";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";

afterEach(() => vi.unstubAllGlobals());

/** The first page with text in a file also loads the renderer, which is slow on a busy machine. */
const COLD_START_MS = 20_000;

const CONFIG = {
  task_prefix: "CS",
  statuses: { task: ["todo", "in-progress", "review", "done", "archived"] },
  priorities: [],
  task_kinds: [],
  fields: {},
};

describe("planning", () => {
  it("milestone_progress_3_of_4", async () => {
    const tasks = ["done", "done", "done", "todo"].map((status, at) => listRow(`tasks/CS-${at + 1}`, { status }));
    fakeApi({
      "/api/items": {
        body: [listRow("milestones/M-1", { type: "milestone", status: "active", title: "Core milestone" }), ...tasks.map((task) => ({ ...task, milestone: "M-1" }))],
      },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search", target: "2026-12-01" } }) },
    });
    renderApp("/milestones");
    const row = (await screen.findByText("Core milestone")).closest("tr");
    expect(row).not.toBeNull();
    expect(await within(row as HTMLElement).findByText("75%")).toBeInTheDocument();
  });

  it("milestone_page_groups_tasks_by_status", async () => {
    const tasks = [
      listRow("tasks/CS-1", { status: "done", title: "Shipped" }),
      listRow("tasks/CS-2", { status: "todo", title: "Waiting" }),
    ];
    fakeApi({
      "/api/config": { body: CONFIG },
      "/api/items": { body: tasks },
      "/api/items?type=task&milestone=M-1": { body: tasks },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" }, body: "The goal text.\n" }) },
    });
    renderApp("/milestones/M-1");
    expect(await screen.findByText("The goal text.", {}, { timeout: COLD_START_MS })).toBeInTheDocument();
    const todo = (await screen.findByRole("heading", { name: /^todo/ })).parentElement as HTMLElement;
    expect(within(todo).getByText("Waiting")).toBeInTheDocument();
    expect(within(todo).queryByText("Shipped")).toBeNull();
  }, COLD_START_MS);

  it("milestone_list_flags_tasks_waiting_on_open_work", async () => {
    const task = (n: number, status: string, depends_on?: string[]) => ({ ...listRow(`tasks/CS-${n}`, { status }), milestone: "M-1", ...(depends_on === undefined ? {} : { depends_on }) });
    const loose = (n: number, status: string) => listRow(`tasks/CS-${n}`, { status });
    fakeApi({
      "/api/items": {
        body: [
          listRow("milestones/M-1", { type: "milestone", status: "active", title: "Core" }),
          task(1, "todo", ["CS-2"]),
          task(2, "in-progress", ["CS-3"]),
          task(3, "done"),
          loose(4, "review"),
          loose(5, "todo"),
          loose(6, "done"),
        ],
      },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1") },
    });
    renderApp("/milestones");
    const row = (await screen.findByText("Core")).closest("tr") as HTMLElement;
    const flag = await within(row).findByText("1 blocked");
    expect(flag.parentElement).toHaveAttribute("title", "CS-1 waits for CS-2");
    expect(await screen.findByText("2 open tasks have no milestone.")).toBeInTheDocument();
    expect(screen.getByText("1 of them wait for review.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "See them on the board" })).toHaveAttribute("href", "/board");
  });

  it("milestone_list_calls_out_a_single_loose_task_with_no_milestones", async () => {
    fakeApi({ "/api/items": { body: [listRow("tasks/CS-1", { status: "todo" })] } });
    renderApp("/milestones");
    expect(await screen.findByText("1 open task has no milestone.")).toBeInTheDocument();
    expect(screen.queryByText(/of them wait for review/)).toBeNull();
  });

  it("milestone_page_shows_plan_and_blockers", async () => {
    const tasks = [listRow("tasks/CS-1", { status: "blocked", title: "Stuck", plan: "plans/core" }), listRow("tasks/CS-2", { status: "done", title: "Shipped", plan: "plans/core" })];
    fakeApi({
      "/api/config": { body: CONFIG },
      "/api/items": { body: [listRow("plans/core", { type: "plan", title: "Core plan", milestone: "M-01" }), listRow("plans/other", { type: "plan", title: "Other plan" }), ...tasks] },
      "/api/items?type=task&milestone=M-1": { body: tasks },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" } }) },
      "/api/items/tasks/CS-1": { body: itemView("tasks/CS-1") },
    });
    renderApp("/milestones/M-1");
    const plans = await screen.findByRole("list", { name: "Plans" });
    expect(within(plans).getByRole("link", { name: "Core plan" })).toHaveAttribute("href", "/plans/core");
    expect(within(plans).getByText("1/2")).toBeInTheDocument();
    expect(within(plans).queryByText("Other plan")).toBeNull();
    const blockers = await screen.findByRole("list", { name: "Blockers" });
    expect(within(blockers).getByRole("link", { name: "Stuck" })).toBeInTheDocument();
    expect(within(blockers).getByText("blocked")).toBeInTheDocument();
  });

  it("milestone_page_says_when_there_is_no_plan_or_blocker", async () => {
    fakeApi({
      "/api/items": { body: [] },
      "/api/items?type=task&milestone=M-1": { body: [] },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" } }) },
    });
    renderApp("/milestones/M-1");
    expect(await screen.findByText("No plan names this milestone yet.")).toBeInTheDocument();
    expect(await screen.findByText("No open task here is blocked.")).toBeInTheDocument();
    expect(screen.getByText("No tasks in this milestone.")).toBeInTheDocument();
  });

  it("plan_lists_tasks", async () => {
    const statuses = ["done", "done", "in-progress", "todo", "review"];
    const rows = statuses.map((status, at) => listRow(`tasks/CS-${at + 1}`, { status, title: `Task ${at + 1}` }));
    const backlinks: Backlink[] = rows.map((row, at) => ({ id: row.id, anchor: null, path: row.path, line: at + 1, kind: "plan" }));
    fakeApi({
      "/api/items": { body: rows },
      "/api/items/plans/m1-core": { body: itemView("plans/m1-core", { fields: { title: "Core" }, body: "Plan text.\n", backlinks }) },
    });
    renderApp("/plans/m1-core");
    const heading = await screen.findByRole("heading", { name: "Tasks (5)" });
    const items = within(heading.parentElement as HTMLElement).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    // What moves comes first, what is over last.
    expect(items.map((item) => item.querySelector("a")?.textContent)).toEqual(["Task 3", "Task 5", "Task 4", "Task 1", "Task 2"]);
    expect(items[0]).toHaveTextContent("in-progress");
  });

  it("plan_list_reads_one_list_and_no_plan", async () => {
    const plan = (slug: string, status: string, over = {}) => listRow(`plans/${slug}`, { type: "plan", status, title: slug, ...over });
    const { calls } = fakeApi({
      "/api/items": {
        body: [
          plan("undated", "active"),
          plan("old", "active", { created: "2026-10-01" }),
          plan("new", "active", { created: "2026-10-12" }),
          plan("finished", "done", { created: "2026-09-01" }),
        ],
      },
    });
    renderApp("/plans");
    const links = await screen.findAllByRole("link", { name: /^(undated|old|new|finished)/ });
    // Newest first, a plan with no day last, the day on the row, and the finished plan out of the first view.
    expect(links.map((link) => link.getAttribute("href"))).toEqual(["/plans/new", "/plans/old", "/plans/undated"]);
    expect(links[0]).toHaveTextContent("new");
    expect(links[0]).toHaveTextContent("2026-10-12");
    expect(screen.queryByRole("link", { name: /finished/ })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /^done/ }));
    expect(screen.getByRole("link", { name: /finished/ })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^new/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^All/ })).toBeNull();
    expect(calls.filter((url) => url.startsWith("/api/items/"))).toEqual([]);
  });

  it("plan_list_opens_on_the_first_status_when_no_plan_is_active", async () => {
    fakeApi({ "/api/items": { body: [listRow("plans/idea", { type: "plan", status: "draft", title: "idea" })] } });
    renderApp("/plans");
    expect(await screen.findByRole("link", { name: /idea/ })).toHaveAttribute("href", "/plans/idea");
  });

  it("plan_list_falls_back_when_the_chosen_status_has_no_plan_left", async () => {
    const plan = (slug: string, status: string) => listRow(`plans/${slug}`, { type: "plan", status, title: slug });
    const routes = { "/api/items": { body: [plan("running", "active"), plan("finished", "done")] } };
    fakeApi(routes);
    const client = createQueryClient();
    render(<App router={createMemoryRouter(client, "/plans")} />);
    await userEvent.click(await screen.findByRole("button", { name: /^done/ }));
    expect(screen.getByRole("link", { name: /finished/ })).toBeInTheDocument();

    // The last done plan moves on: the chosen chip is gone, and the list opens on the first rule again.
    routes["/api/items"] = { body: [plan("running", "active"), plan("finished", "active")] };
    void client.invalidateQueries({ queryKey: ["items", "list"] });
    expect(await screen.findByRole("link", { name: /running/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /finished/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^done/ })).toBeNull();
    expect(screen.getByRole("button", { name: /^active/ })).toHaveAttribute("aria-pressed", "true");
  }, COLD_START_MS);

  it("a_task_opens_over_the_page_from_its_row", async () => {
    const row = listRow("tasks/CS-1", { status: "todo", title: "Peek me", milestone: "M-1" });
    // The criteria and the outcome come from the server, not from the body, so the body holds neither.
    const task = itemView("tasks/CS-1", {
      body: "## Description\n\nThe body of the task.\n",
      criteria: [
        { n: 1, text: "First check", checked: true },
        { n: 2, text: "Second check", checked: false },
        { n: 3, text: "Third check", checked: false },
      ],
      outcome: "It works as planned.\n\n### Details\n\nDetail under a sub-heading.",
    });
    fakeApi({
      "/api/items": { body: [row] },
      "/api/items?type=task&milestone=M-1": { body: [row] },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" } }) },
      "/api/items/tasks/CS-1": { body: task },
    });
    const router = renderApp("/milestones/M-1");
    await userEvent.click(await screen.findByRole("link", { name: "Peek me" }));

    const dialog = await screen.findByRole("dialog", { name: "Peek me" });
    expect(await within(dialog).findByText("The body of the task.")).toBeInTheDocument();
    expect(within(dialog).getByText("Second check")).toBeInTheDocument();
    expect(within(dialog).getByText("1 of 3")).toBeInTheDocument();
    expect(within(dialog).getByText("It works as planned.")).toBeInTheDocument();
    expect(within(dialog).getByText("Detail under a sub-heading.")).toBeInTheDocument();
    const facts = within(dialog).getByLabelText("Facts");
    expect(within(facts).getByText("Milestone")).toBeInTheDocument();
    expect(within(facts).queryByText("Plan")).toBeNull();
    expect(within(dialog).getByRole("link", { name: "Open full page" })).toHaveAttribute("href", "/tasks/CS-1");
    expect(router.state.location.pathname).toBe("/milestones/M-1");

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("the_owner_acts_on_a_task_in_review", async () => {
    const row = listRow("tasks/CS-1", { status: "review", title: "Peek me", milestone: "M-1" });
    const routes = {
      "/api/config": { body: CONFIG },
      "/api/items": { body: [row] },
      "/api/items?type=task&milestone=M-1": { body: [row] },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" } }) },
      "/api/items/tasks/CS-1": { body: itemView("tasks/CS-1", { fields: { title: "Peek me", status: "review" }, body: "## Description\n\nThe body of the task.\n" }) },
    };
    const sent = (mock: ReturnType<typeof fakeApi>["fetchMock"]) => {
      const call = mock.mock.calls.find(([, init]) => init?.method === "PATCH");
      return call === undefined ? null : { url: String(call[0]), body: JSON.parse(String(call[1]?.body)) };
    };
    const open = async () => {
      await userEvent.click(await screen.findByRole("link", { name: "Peek me" }));
      return screen.findByRole("dialog", { name: "Peek me" });
    };
    renderApp("/milestones/M-1");

    // A refused write shows the server's sentence and keeps the dialog open.
    fakeApi({ ...routes, "PATCH /api/items/tasks/CS-1": { status: 409, body: { error: "tasks/CS-1 changed on disk; reload", kind: "conflict" } } });
    let dialog = await open();
    await userEvent.click(await within(dialog).findByRole("button", { name: "Accept" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("tasks/CS-1 changed on disk; reload");
    expect(screen.getByRole("dialog", { name: "Peek me" })).toBeInTheDocument();

    const accepted = fakeApi({ ...routes, "PATCH /api/items/tasks/CS-1": { body: { id: "tasks/CS-1", changed: true, etag: "next" } } });
    await userEvent.click(within(dialog).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sent(accepted.fetchMock)).toEqual({ url: "/api/items/tasks/CS-1", body: { set: { status: "done" }, if_match: "etag" } });

    const back = fakeApi({ ...routes, "PATCH /api/items/tasks/CS-1": { body: { id: "tasks/CS-1", changed: true, etag: "next" } } });
    dialog = await open();
    await userEvent.click(await within(dialog).findByRole("button", { name: "Send back" }));
    const submit = within(dialog).getAllByRole("button", { name: "Send back" }).at(-1) as HTMLElement;
    expect(submit).toBeDisabled();
    await userEvent.type(within(dialog).getByRole("textbox", { name: "What has to change" }), "fix the title");
    expect(submit).toBeEnabled();
    await userEvent.click(submit);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sent(back.fetchMock)?.body).toEqual({ set: { status: "todo" }, note: "Sent back: fix the title", if_match: "etag" });
  });

  it("a_task_outside_review_has_no_decision_buttons", async () => {
    const row = listRow("tasks/CS-1", { status: "todo", title: "Peek me", milestone: "M-1" });
    fakeApi({
      "/api/config": { body: CONFIG },
      "/api/items": { body: [row] },
      "/api/items?type=task&milestone=M-1": { body: [row] },
      "/api/items/milestones/M-1": { body: itemView("milestones/M-1", { fields: { title: "Search" } }) },
      "/api/items/tasks/CS-1": { body: itemView("tasks/CS-1", { fields: { title: "Peek me", status: "todo" } }) },
    });
    renderApp("/milestones/M-1");
    await userEvent.click(await screen.findByRole("link", { name: "Peek me" }));
    const dialog = await screen.findByRole("dialog", { name: "Peek me" });
    expect(await within(dialog).findByRole("combobox", { name: "Status" })).toHaveValue("todo");
    expect(within(dialog).queryByRole("button", { name: "Accept" })).toBeNull();
    expect(within(dialog).queryByRole("button", { name: "Send back" })).toBeNull();
  });

  it("resting_on_a_link_starts_loading_its_page", async () => {
    const row = listRow("plans/one", { type: "plan", title: "One" });
    const { calls } = fakeApi({ "/api/items": { body: [row] }, "/api/items/plans/one": { body: itemView("plans/one") } });
    renderApp("/plans");
    await userEvent.hover(await screen.findByRole("link", { name: "One" }));
    await waitFor(() => expect(calls).toContain("/api/items/plans/one"));
  });
});
