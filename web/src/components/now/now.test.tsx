import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeApi } from "@/test/fakeApi";
import { listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { prdOf } from "./firstRunModel";

afterEach(() => vi.unstubAllGlobals());

const TASKS = [
  listRow("tasks/CS-1", { status: "review", milestone: "M-1", title: "Pay by card", etag: "e1" }),
  listRow("tasks/CS-2", { status: "review", milestone: "M-1", title: "Refund a payment" }),
  listRow("tasks/CS-3", { status: "done", milestone: "M-1" }),
  listRow("tasks/CS-4", { status: "done", milestone: "M-1" }),
  listRow("tasks/CS-5", { status: "in-progress" }),
  listRow("tasks/CS-6", { status: "todo" }),
  listRow("tasks/CS-7", { status: "todo" }),
  listRow("tasks/CS-8", { status: "done" }),
];
const MILESTONES = [listRow("milestones/M-1", { type: "milestone", status: "active", title: "First release" }), listRow("milestones/M-2", { type: "milestone", status: "done", title: "Old release" })];

const FINDING = { kind: "code", from: "arch/shop/api", to: "arch/shop/web", path: "stone/arch/shop/api.md", line: 3, rule: "arch-drift", message: "Docs say Api calls Web; the code shows no such link" };

function home(graph: { status?: number; body: unknown }) {
  return fakeApi({
    "/api/items": { body: [...TASKS, ...MILESTONES] },
    "/api/arch/graph": graph,
    "PATCH /api/items/tasks/CS-1": { body: { id: "tasks/CS-1", changed: true, etag: "next" } },
  });
}

const GRAPH = { nodes: [], uses: [], code_edges: [], diagrams: [], drift: [FINDING] };

function tile(label: RegExp) {
  return screen.getByRole("link", { name: label });
}

describe("now page", () => {
  it("the_home_page_says_what_needs_the_owner", async () => {
    home({ body: GRAPH });
    renderApp("/");
    expect(await screen.findByRole("heading", { level: 1, name: "Now" })).toBeInTheDocument();
    await screen.findByText("Refund a payment");
    expect(tile(/waiting for your review/)).toHaveTextContent("2waiting for your review");
    expect(tile(/in progress/)).toHaveTextContent("1in progress");
    expect(tile(/ready to start/)).toHaveTextContent("2ready to start");
    // CS-5, CS-6 and CS-7 are open with no milestone; the finished CS-8 is not.
    expect(tile(/no milestone/)).toHaveTextContent("3open with no milestone");
    expect(tile(/no milestone/)).toHaveAttribute("href", "/milestones");

    const queue = screen.getByRole("heading", { name: "Needs your decision" }).closest("section") as HTMLElement;
    expect(within(queue).getAllByRole("button", { name: "Accept" })).toHaveLength(2);
    expect(within(queue).getAllByRole("button", { name: "Send back" })).toHaveLength(2);
    expect(within(queue).getByRole("link", { name: "Pay by card" })).toBeInTheDocument();

    expect(await screen.findByRole("link", { name: FINDING.message })).toHaveAttribute("href", "/architecture");
    // Two of the four tasks of M-1 are done; the finished milestone is not listed.
    const milestone = screen.getByRole("link", { name: /First release/ });
    expect(milestone).toHaveAttribute("href", "/milestones/M-1");
    expect(milestone).toHaveTextContent("50%");
    expect(screen.queryByText("Old release")).toBeNull();
  });

  it("accepting_from_the_queue_writes_done", async () => {
    const { fetchMock } = home({ body: GRAPH });
    renderApp("/");
    const row = (await screen.findByRole("link", { name: "Pay by card" })).parentElement as HTMLElement;
    await userEvent.click(within(row).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(fetchMock.mock.calls.some(([, init]) => init?.method === "PATCH")).toBe(true));
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === "PATCH");
    expect(String(call?.[0])).toBe("/api/items/tasks/CS-1");
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ set: { status: "done" }, if_match: "e1" });
  });

  it("a_project_with_no_design_still_opens", async () => {
    const { calls } = home({ status: 404, body: { error: "no design", kind: "not_found" } });
    renderApp("/");
    await screen.findByText("Refund a payment");
    await waitFor(() => expect(calls).toContain("/api/arch/graph"));
    // One more turn of the event loop, so the refused answer has been read.
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(tile(/waiting for your review/)).toHaveTextContent("2waiting for your review");
    expect(screen.queryByRole("heading", { name: "Something looks wrong" })).toBeNull();
    expect(screen.getByRole("heading", { name: "Milestones" })).toBeInTheDocument();
  });
});

const PRD = listRow("docs/prd", { type: "doc", title: "Product requirements", etag: "prd1" });
const NOTES = listRow("docs/notes", { type: "doc", title: "Notes" });
const DECISION = listRow("decisions/ADR-1", { type: "decision", title: "Use Rust" });
const HLD = { systems: [], containers: [{ id: "arch/shop/api" }, { id: "arch/shop/web" }, { id: "arch/shop/db" }], connections: [], drawn: [], docs: [] };
const OVERVIEW = { id: "docs/product-overview", title: "Shop overview", summary: "A shop." };

function firstRun(over: Record<string, { status?: number; body: unknown }> = {}) {
  const routes: Record<string, { status?: number; body: unknown }> = {
    "/api/items": { body: [...TASKS, ...MILESTONES] },
    "/api/items?type=doc": { body: [PRD, NOTES] },
    "/api/items?type=decision": { body: [DECISION] },
    "/api/arch/graph": { body: GRAPH },
    "/api/arch/hld": { body: HLD },
    "/api/product": { body: { overview: null, features: [] } },
    "PATCH /api/items/docs/prd": { body: { id: "docs/prd", changed: true, etag: "prd2" } },
    ...over,
  };
  return { routes, ...fakeApi(routes) };
}

const panel = () => screen.queryByRole("region", { name: "Stone already knows this much" });

describe("prdOf", () => {
  const cases: [string, ReturnType<typeof listRow>[], string | undefined][] = [
    ["the doc with the id docs/prd wins over a better title", [listRow("docs/spec", { type: "doc", title: "PRD v2" }), PRD], "docs/prd"],
    ["a title with PRD, in any case", [NOTES, listRow("docs/req", { type: "doc", title: "Shop prd draft" })], "docs/req"],
    ["a title with product requirements, in any case", [listRow("docs/req", { type: "doc", title: "Product Requirements" })], "docs/req"],
    ["a task titled PRD is not a doc", [listRow("tasks/CS-1", { title: "Write the PRD" })], undefined],
    ["no doc, no answer", [NOTES], undefined],
  ];
  it.each(cases)("%s", (_name, rows, id) => {
    expect(prdOf(rows)?.id).toBe(id);
  });
});

describe("first run panel", () => {
  afterEach(() => localStorage.clear());

  it("a_project_with_no_overview_shows_what_stone_found", async () => {
    firstRun();
    renderApp("/");
    const section = (await screen.findByRole("region", { name: "Stone already knows this much" })) as HTMLElement;
    const found = (label: string) => within(section).getByText(label).parentElement;
    expect(found("tasks")).toHaveTextContent("8tasks");
    expect(found("milestones")).toHaveTextContent("2milestones");
    expect(found("decisions")).toHaveTextContent("1decisions");
    expect(found("docs")).toHaveTextContent("2docs");
    expect(within(section).getByText("Use the PRD as the product overview")).toBeInTheDocument();
    expect(within(section).getByText(/Product requirements becomes the overview/)).toBeInTheDocument();
    expect(within(section).getByText("List your features")).toBeInTheDocument();
    expect(within(section).getByText("3 services are on the System map.")).toBeInTheDocument();
    expect(within(section).getByRole("link", { name: "Review services" })).toHaveAttribute("href", "/architecture");
  });

  it("use_prd_writes_the_kind_with_the_etag_and_the_panel_leaves", async () => {
    const { routes, fetchMock } = firstRun();
    renderApp("/");
    const button = await screen.findByRole("button", { name: "Use PRD" });
    // The product the refetch after the write reads has an overview.
    routes["/api/product"] = { body: { overview: OVERVIEW, features: [] } };
    await userEvent.click(button);
    await waitFor(() => expect(fetchMock.mock.calls.some(([, init]) => init?.method === "PATCH")).toBe(true));
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === "PATCH");
    expect(String(call?.[0])).toBe("/api/items/docs/prd");
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ set: { kind: "product-overview" }, if_match: "prd1" });
    await waitFor(() => expect(panel()).toBeNull());
  });

  it("a_refused_write_is_shown_under_the_step", async () => {
    firstRun({ "PATCH /api/items/docs/prd": { status: 409, body: { error: "docs/prd changed on disk. Read it again.", kind: "conflict" } } });
    renderApp("/");
    await userEvent.click(await screen.findByRole("button", { name: "Use PRD" }));
    expect(await screen.findByText(/changed on disk/)).toHaveAttribute("role", "alert");
    expect(panel()).not.toBeNull();
  });

  it("with_an_overview_there_is_no_panel", async () => {
    firstRun({ "/api/product": { body: { overview: OVERVIEW, features: [] } } });
    renderApp("/");
    await screen.findByText("Refund a payment");
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(panel()).toBeNull();
  });

  it("with_no_prd_the_step_is_gone", async () => {
    firstRun({ "/api/items?type=doc": { body: [NOTES] } });
    renderApp("/");
    await screen.findByRole("region", { name: "Stone already knows this much" });
    expect(screen.queryByText("Use the PRD as the product overview")).toBeNull();
    expect(screen.getByText("List your features")).toBeInTheDocument();
  });

  it("hide_removes_the_panel_and_a_new_visit_keeps_it_hidden", async () => {
    firstRun();
    renderApp("/");
    await userEvent.click(await screen.findByRole("button", { name: "Hide" }));
    expect(panel()).toBeNull();
    cleanup();

    renderApp("/");
    await screen.findByText("Refund a payment");
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(panel()).toBeNull();
  });

  it("copy_prompt_writes_the_prompt_and_says_so", async () => {
    firstRun();
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    renderApp("/");
    await userEvent.click(await screen.findByRole("button", { name: "Copy prompt" }));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("stone get docs/prd"));
  });

  it("a_refused_clipboard_shows_the_prompt_to_copy_by_hand", async () => {
    firstRun();
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText: () => Promise.reject(new Error("denied")) } });
    renderApp("/");
    await userEvent.click(await screen.findByRole("button", { name: "Copy prompt" }));
    expect(await screen.findByText(/stone new doc --set kind=feature/)).toBeInTheDocument();
  });

  it("a_failed_product_read_shows_no_panel_and_now_still_draws", async () => {
    firstRun({ "/api/product": { status: 500, body: { error: "boom", kind: "internal" } } });
    renderApp("/");
    await screen.findByText("Refund a payment");
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(panel()).toBeNull();
    expect(tile(/waiting for your review/)).toHaveTextContent("2waiting for your review");
  });
});
