import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ActivityGroup } from "@/api/types/ActivityGroup";
import type { TaskCommit } from "@/api/types/TaskCommit";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { earlier, topicsOf } from "./activityModel";
import { readTask } from "./taskModel";

afterEach(() => vi.unstubAllGlobals());

/** The first page with text in a file also loads the renderer, which is slow on a busy machine. */
const COLD_START_MS = 20_000;

const ROWS = [
  listRow("tasks/CS-1", { title: "The task", status: "in-progress", milestone: "M-1" }),
  listRow("tasks/CS-2", { title: "Needed first", status: "todo" }),
  listRow("tasks/CS-3", { title: "Already done", status: "done" }),
  listRow("tasks/CS-4", { title: "Waits for this", status: "todo" }),
  listRow("milestones/M-1", { type: "milestone", title: "Core" }),
  listRow("plans/core", { type: "plan", title: "Core plan" }),
  listRow("docs/feature-pay", { title: "Pay" }),
  listRow("docs/notes", { title: "Notes" }),
  listRow("arch/api", { type: "container", title: "Api" }),
];

const TASK = itemView("tasks/CS-1", {
  fields: { title: "The task", status: "in-progress", priority: "high", depends_on: ["CS-2", "CS-3"], milestone: "M-1", plan: "[[plans/core]]", links: ["docs/notes", "docs/feature-pay", "arch/api"] },
  body: "## Description\n\nMake it quick for everyone.\n\nA second paragraph.\n\n## Acceptance criteria\n\n- [x] First part\n- [ ] Second part\n\n## Notes\n\n```\n- [ ] not a box\n```\n",
  backlinks: [{ id: "tasks/CS-4", anchor: null, path: "stone/tasks/CS-4.md", line: 4, kind: "depends_on" }],
  changed_files: [{ path: "src/pay.rs", status: "M", source: "refs" }],
});

const COMMITS: TaskCommit[] = [
  { sha: "1111111aaaa", repo: "", date: "2026-10-30T10:00:00Z", title: "feat: part one", how: "refs" },
  { sha: "2222222bbbb", repo: "backend", date: "2026-10-29T10:00:00Z", title: "fix: guessed", how: "path" },
];

const FIRST_GAP = "components is empty; set it with `stone edit CS-1 --set components=<arch id>`";
const SECOND_GAP = "paths is empty; set it with `stone edit CS-1 --set paths=<file>,<file>`";

function task(extra = {}) {
  return fakeApi({
    "/api/items": { body: ROWS },
    "/api/items/tasks/CS-1": { body: TASK },
    "/api/tasks/CS-1/commits": { body: COMMITS },
    "/api/tasks/CS-1/features": { body: [{ id: "docs/feature-pay", title: "Pay" }] },
    "/api/product": { body: { overview: null, features: [{ id: "docs/feature-pay", title: "Pay", platforms: [], open_tasks: 1, done_tasks: 0, inferred_open_tasks: 0 }] } },
    ...extra,
  });
}

function fact(label: string) {
  return within(screen.getByRole("definition", { name: label }));
}

describe("task page", () => {
  it("top_says_where_the_task_stands", async () => {
    const { fetchMock } = task({
      "/api/config": { body: { task_prefix: "CS", statuses: { task: ["todo", "in-progress", "review", "done"] }, priorities: [], task_kinds: [], fields: {} } },
      "PATCH /api/items/tasks/CS-1": { body: { id: "tasks/CS-1", changed: true, etag: "next" } },
    });
    renderApp("/tasks/CS-1");
    expect(await screen.findByRole("heading", { level: 1, name: "The task" }, { timeout: COLD_START_MS })).toBeInTheDocument();
    expect(fact("Status").getByText("in-progress")).toBeInTheDocument();
    expect(fact("Goal").getByText("Make it quick for everyone.")).toBeInTheDocument();
    expect(fact("Goal").queryByText("A second paragraph.")).toBeNull();
    expect(fact("What is left").getByText("1 of 2 checklist items open")).toBeInTheDocument();
    expect(fact("What is left").getByText("Second part")).toBeInTheDocument();
    expect(fact("What is left").queryByText("not a box")).toBeNull();
    await waitFor(() => expect(fact("Milestone").getByRole("link", { name: "Core" })).toHaveAttribute("href", "/milestones/M-1"));
    expect(fact("Plan").getByRole("link", { name: "Core plan" })).toHaveAttribute("href", "/plans/core");
    expect(await fact("Feature").findByRole("link", { name: "Pay" })).toHaveAttribute("href", "/features/feature-pay");

    // Choosing a status in the select writes it with the etag this page was read at.
    await fact("Status").findByRole("option", { name: "In review" });
    await userEvent.selectOptions(fact("Status").getByRole("combobox", { name: "Status" }), "review");
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === "PATCH");
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ set: { status: "review" }, if_match: "etag" });
  }, COLD_START_MS);

  it("the_status_picker_offers_the_override", async () => {
    const bare = itemView("tasks/CS-1", { fields: { title: "The task", status: "todo" } });
    const { fetchMock } = task({
      "/api/items/tasks/CS-1": { body: bare },
      "/api/config": { body: { task_prefix: "CS", statuses: { task: ["todo", "in-progress", "review", "done"] }, priorities: [], task_kinds: [], fields: {} } },
      "PATCH /api/items/tasks/CS-1": { body: { id: "tasks/CS-1", changed: true, etag: "next" } },
    });
    // The first write is refused; the second passes.
    const answer = fetchMock.getMockImplementation();
    let refused = false;
    fetchMock.mockImplementation((input, init) => {
      if (init?.method === "PATCH" && !refused) {
        refused = true;
        const error = `CS-1 is not ready for in-progress: ${FIRST_GAP}; ${SECOND_GAP}; fix them, or run stone edit CS-1 --set status=in-progress --force`;
        return Promise.resolve(new Response(JSON.stringify({ error, kind: "invalid" }), { status: 422, headers: { "Content-Type": "application/json" } }));
      }
      return answer?.(input, init) as Promise<Response>;
    });
    renderApp("/tasks/CS-1");
    await screen.findByRole("heading", { level: 1, name: "The task" }, { timeout: COLD_START_MS });
    await fact("Status").findByRole("option", { name: "In progress" });
    await userEvent.selectOptions(fact("Status").getByRole("combobox", { name: "Status" }), "in-progress");

    // Both gaps show unchanged in one paragraph, even though a gap holds `; `; the CLI hint stays out.
    const alert = await screen.findByText("This task is not ready to start:");
    expect(within(alert.parentElement as HTMLElement).getByText(`${FIRST_GAP}; ${SECOND_GAP}`)).toBeInTheDocument();
    expect(screen.queryByText(/fix them, or run/)).toBeNull();
    expect(screen.queryByText(/--force/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Start anyway" }));
    await waitFor(() => expect(fetchMock.mock.calls.filter(([, init]) => init?.method === "PATCH")).toHaveLength(2));
    const second = fetchMock.mock.calls.filter(([, init]) => init?.method === "PATCH")[1];
    expect(JSON.parse(String(second?.[1]?.body))).toEqual({ set: { status: "in-progress" }, force: true, if_match: "etag" });
  }, COLD_START_MS);

  it("blocked_by_names_what_is_still_open_and_blocks_reads_backlinks", async () => {
    task();
    renderApp("/tasks/CS-1");
    await screen.findByRole("heading", { level: 1, name: "The task" });
    const blockedBy = await waitFor(() => {
      const found = fact("Blocked by");
      expect(found.getByRole("link", { name: "Needed first" })).toHaveAttribute("href", "/tasks/CS-2");
      return found;
    });
    expect(blockedBy.queryByRole("link", { name: "Already done" })).toBeNull();
    expect(blockedBy.getByText(/1 task it needed is finished/)).toBeInTheDocument();
    expect(fact("Blocks").getByRole("link", { name: "Waits for this" })).toHaveAttribute("href", "/tasks/CS-4");
  });

  it("commits_say_how_they_were_linked", async () => {
    const { calls } = task();
    renderApp("/tasks/CS-1");
    const rows = within(await screen.findByRole("list", { name: "Commits" })).getAllByRole("listitem");
    expect(calls).toContain("/api/tasks/CS-1/commits");
    expect(rows[0]).toHaveTextContent("Refs line");
    expect(rows[1]).toHaveTextContent("Same files");
    expect(rows[1]).toHaveTextContent("backend");
  });

  it("docs_and_architecture_sit_below_and_the_text_closes_the_page", async () => {
    task();
    renderApp("/tasks/CS-1");
    const docs = await screen.findByRole("list", { name: "Docs" });
    expect(await within(docs).findByRole("link", { name: "Notes" })).toHaveAttribute("href", "/docs/notes");
    const architecture = await screen.findByRole("list", { name: "Architecture" });
    expect(within(architecture).getByRole("link", { name: "Api" })).toHaveAttribute("href", "/architecture/arch/api");
    expect(within(screen.getByRole("region", { name: "Task text" })).getByRole("heading", { name: /Acceptance criteria/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open as item" })).toHaveAttribute("href", "/items/tasks/CS-1");
  });

  it("shows_an_inferred_feature_from_one_answer", async () => {
    const bare = itemView("tasks/CS-1", { fields: { title: "The task", status: "todo" } });
    task({
      "/api/items/tasks/CS-1": { body: bare },
      "/api/tasks/CS-1/features": { body: [{ id: "docs/feature-pay", title: "Pay", via: "a tag" }] },
    });
    renderApp("/tasks/CS-1");
    await screen.findByRole("heading", { level: 1, name: "The task" });
    expect(await fact("Feature").findByRole("link", { name: "Pay" })).toBeInTheDocument();
    expect(fact("Feature").getByText("Related")).toBeInTheDocument();
  });

  it("an_unlinked_done_task_reads_plainly", async () => {
    const bare = itemView("tasks/CS-1", { fields: { title: "The task", status: "done" } });
    task({ "/api/items/tasks/CS-1": { body: bare }, "/api/tasks/CS-1/commits": { body: [] }, "/api/tasks/CS-1/features": { body: [] }, "/api/product": { body: { overview: null, features: [] } } });
    renderApp("/tasks/CS-1");
    await screen.findByRole("heading", { level: 1, name: "The task" });
    expect(fact("What is left").getByText("Nothing. This task is done.")).toBeInTheDocument();
    expect(await fact("Feature").findByText("This project lists no features yet.")).toBeInTheDocument();
    expect(await screen.findByText(/No commit names this task/)).toBeInTheDocument();
    expect(screen.getByText(/No doc links this task/)).toBeInTheDocument();
    expect(screen.getByText(/names no part of the architecture/)).toBeInTheDocument();
  });

  it("the_old_item_page_stays_reachable", async () => {
    task();
    renderApp("/items/tasks/CS-1");
    expect(await screen.findByRole("tab", { name: "Provenance" })).toBeInTheDocument();
  });
});

describe("task text", () => {
  it("goal_prefers_a_goal_section", () => {
    expect(readTask("Intro.\n\n## Description\n\nThe description.\n\n## Goal\n\nThe goal\nwrapped.\n").goal).toBe("The goal\nwrapped.");
  });

  it("goal_falls_back_to_the_opening", () => {
    expect(readTask("Just text.\n\n## Other\n\nMore.\n").goal).toBe("Just text.");
    expect(readTask("## Other\n\nMore.\n").goal).toBeNull();
  });
});

const GROUPS: ActivityGroup[] = [
  {
    label: "Week of 26 October 2026",
    start: "2026-10-26",
    tasks_done: [
      { id: "CS-5", title: "Exact date", closed: "2026-10-27", estimated: false },
      { id: "CS-6", title: "Guessed date", closed: "2026-10-28", estimated: true },
    ],
    docs_changed: [{ id: "docs/prd", title: "Requirements", day: "2026-10-27" }],
    decisions_changed: [{ id: "decisions/D-1", title: "Use one store", day: "2026-10-26" }],
    commit_count: 12,
  },
  { label: "Week of 19 October 2026", start: "2026-10-19", tasks_done: [], docs_changed: [], decisions_changed: [], commit_count: 1 },
];

describe("activity", () => {
  const WEEK = "/api/activity?period=week";

  it("shows_a_week_by_default", async () => {
    const items = [listRow("milestones/M-1", { type: "milestone", title: "Core" }), listRow("tasks/CS-5", { title: "Exact date", milestone: "M-1" })];
    fakeApi({ "/api/items": { body: items }, [WEEK]: { body: GROUPS } });
    renderApp("/activity");
    const group = await screen.findByRole("region", { name: "Week of 26 October 2026" });
    expect(screen.getByRole("heading", { level: 1, name: "Shipped" })).toBeInTheDocument();
    expect(await within(group).findByRole("heading", { level: 3, name: /^Core\b/ })).toBeInTheDocument();
    expect(within(group).getByRole("heading", { level: 3, name: /^No milestone\b/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Week" })).toHaveAttribute("aria-selected", "true");
    expect(within(group).getByText("2 tasks finished · 1 doc changed · 1 decision changed · 12 commits")).toBeInTheDocument();
    expect(within(group).getByRole("link", { name: "Exact date" })).toHaveAttribute("href", "/tasks/CS-5");
    expect(within(group).getByRole("link", { name: "Requirements" })).toHaveAttribute("href", "/docs/prd");
    expect(within(group).getByRole("link", { name: "Use one store" })).toHaveAttribute("href", "/items/decisions/D-1");
    expect(within(screen.getByRole("region", { name: "Week of 19 October 2026" })).getByText(/1 commit$/)).toBeInTheDocument();
  });

  it("shows_every_finished_task_with_its_day_and_no_estimate_mark", async () => {
    fakeApi({ "/api/items": { body: [] }, [WEEK]: { body: GROUPS } });
    renderApp("/activity");
    const exact = (await screen.findByRole("link", { name: "Exact date" })).closest("li") as HTMLElement;
    const guessed = screen.getByRole("link", { name: "Guessed date" }).closest("li") as HTMLElement;
    expect(within(exact).getByText("2026-10-27")).toBeInTheDocument();
    expect(within(guessed).getByText("2026-10-28")).toBeInTheDocument();
    expect(screen.queryByText(/estimated/)).toBeNull();
  });

  it("tabs_ask_for_another_period_and_keep_it_in_the_address", async () => {
    const { calls } = fakeApi({
      "/api/items": { body: [] },
      [WEEK]: { body: GROUPS },
      "/api/activity?period=month": { body: [{ ...GROUPS[1], label: "October 2026", start: "2026-10-01" }] },
    });
    const router = renderApp("/activity");
    await screen.findByRole("region", { name: "Week of 26 October 2026" });
    await userEvent.click(screen.getByRole("tab", { name: "Month" }));
    expect(await screen.findByRole("region", { name: "October 2026" })).toBeInTheDocument();
    expect(calls).toContain("/api/activity?period=month");
    expect(screen.queryByRole("region", { name: "Week of 26 October 2026" })).toBeNull();
    await waitFor(() => expect(router.state.location.search).toMatchObject({ period: "month" }));
  });

  it("show_earlier_reads_further_back", async () => {
    const { calls } = fakeApi({
      "/api/items": { body: [] },
      [WEEK]: { body: GROUPS },
      "/api/activity?period=week&from=2026-08-24": { body: [...GROUPS, { ...GROUPS[1], label: "Week of 24 August 2026", start: "2026-08-24" }] },
    });
    renderApp("/activity");
    await screen.findByRole("region", { name: "Week of 19 October 2026" });
    await userEvent.click(screen.getByRole("button", { name: "Show earlier" }));
    expect(await screen.findByRole("region", { name: "Week of 24 August 2026" })).toBeInTheDocument();
    expect(calls).toContain("/api/activity?period=week&from=2026-08-24");
    expect(screen.getByRole("region", { name: "Week of 26 October 2026" })).toBeInTheDocument();
  });

  it("show_earlier_gets_past_a_quiet_stretch", async () => {
    const quiet = "/api/activity?period=week&from=2026-08-24";
    const { calls } = fakeApi({ "/api/items": { body: [] }, [quiet]: { body: GROUPS }, "/api/activity": { body: [] } });
    renderApp("/activity?period=week&from=2026-08-24");
    await screen.findByRole("region", { name: "Week of 19 October 2026" });
    await userEvent.click(screen.getByRole("button", { name: "Show earlier" }));
    await waitFor(() => expect(calls).toContain("/api/activity?period=week&from=2026-06-29"));
  });

  it("an_empty_stretch_says_so", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/activity": { body: [] } });
    renderApp("/activity?period=day");
    expect(await screen.findByText("Nothing was finished or changed in this stretch.")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Day" })).toHaveAttribute("aria-selected", "true");
  });

  it("a_failed_item_list_shows_a_notice_and_keeps_the_tasks", async () => {
    // No `/api/items` route, so the fake answers 404.
    fakeApi({ [WEEK]: { body: GROUPS } });
    renderApp("/activity");
    const group = await screen.findByRole("region", { name: "Week of 26 October 2026" });
    expect(await screen.findByRole("alert")).toHaveTextContent("/api/items not found");
    expect(within(group).getByRole("heading", { level: 3, name: /^No milestone\b/ })).toBeInTheDocument();
    expect(within(group).getByRole("link", { name: "Exact date" })).toBeInTheDocument();
  }, 20_000);

  it("no_task_reads_as_without_a_milestone_while_the_item_list_loads", async () => {
    let release: (reply: Response) => void = () => {};
    const items = new Promise<Response>((resolve) => {
      release = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/items") return items;
        const body = url.startsWith("/api/activity") ? GROUPS : {};
        return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } }));
      }),
    );
    renderApp("/activity");
    const group = await screen.findByRole("region", { name: "Week of 26 October 2026" });
    expect(within(group).queryByRole("heading", { level: 3, name: /^(Core|No milestone)\b/ })).toBeNull();
    expect(within(group).getByText("Reading the milestones...")).toBeInTheDocument();

    const rows = [listRow("milestones/M-1", { type: "milestone", title: "Core" }), listRow("tasks/CS-5", { title: "Exact date", milestone: "M-1" })];
    release(new Response(JSON.stringify(rows), { status: 200, headers: { "Content-Type": "application/json" } }));
    expect(await within(group).findByRole("heading", { level: 3, name: /^Core\b/ })).toBeInTheDocument();
    expect(within(group).queryByText("Reading the milestones...")).toBeNull();
  });

  it("topics_group_finished_tasks_by_milestone", () => {
    const row = (n: number, milestone?: string) => listRow(`tasks/CS-${n}`, milestone === undefined ? {} : { milestone });
    const rows = [row(1, "M-1"), row(2, "[[milestones/M-02.md]]"), row(3, "M-2"), row(4), row(5, "M-9"), row(7, "M-3")];
    const milestones = ["Core", "Search", "Alpha"].map((title, at) => listRow(`milestones/M-${at + 1}`, { type: "milestone", title }));
    const done = [1, 2, 3, 4, 5, 6, 7].map((n) => ({ id: `CS-${n}`, title: `Task ${n}`, closed: "2026-10-27", estimated: false }));
    const seen = topicsOf(done, rows, milestones).map((topic) => [topic.name, topic.rows.map(({ task, row }) => [task.id, row?.id])]);
    expect(seen).toEqual([
      ["Search", [["CS-2", "tasks/CS-2"], ["CS-3", "tasks/CS-3"]]],
      ["Alpha", [["CS-7", "tasks/CS-7"]]],
      ["Core", [["CS-1", "tasks/CS-1"]]],
      ["No milestone", [["CS-4", "tasks/CS-4"], ["CS-5", "tasks/CS-5"], ["CS-6", undefined]]],
    ]);
  });

  it("earlier_steps_back_a_whole_stretch", () => {
    expect(earlier("day", "2026-11-04")).toBe("2026-10-21");
    expect(earlier("week", "2026-11-02")).toBe("2026-09-07");
    expect(earlier("month", "2026-11-01")).toBe("2026-05-01");
  });
});
