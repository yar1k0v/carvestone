import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ArchGraph } from "@/api/types/ArchGraph";
import type { DomainModel } from "@/api/types/DomainModel";
import type { Explain } from "@/api/types/Explain";
import type { Hld } from "@/api/types/Hld";
import type { Lld } from "@/api/types/Lld";
import type { Module } from "@/api/types/Module";
import { installFlowMocks } from "@/components/graph/flowMocks";
import { fakeApi, type Reply } from "@/test/fakeApi";
import { listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { codeConnections, connectionRows, driftRows, moduleGroups, modulesIn, runningOn, servicesOf } from "./model";

beforeEach(() => installFlowMocks());
afterEach(() => vi.unstubAllGlobals());

const HLD: Hld = {
  systems: [{ id: "arch/shop", title: "Shop", type: "system", description: "Sells things" }],
  containers: [
    { id: "arch/api", title: "API", type: "container", description: "Takes orders", technology: "ASP.NET", parent: "arch/shop" },
    { id: "arch/db", title: "Store", type: "container", parent: "arch/shop" },
    { id: "arch/web", title: "Web", type: "container", parent: "arch/shop" },
  ],
  connections: [
    { from: "arch/web", to: "arch/api" },
    { from: "arch/api", to: "arch/db", description: "keeps orders" },
  ],
  drawn: [],
  docs: [{ id: "docs/shape", title: "Shape of the product" }],
};

const GRAPH: ArchGraph = { nodes: [], uses: [], code_edges: [], drift: [], diagrams: [] };

const EXPLAIN: Explain = {
  target: "arch/api",
  entries: [{ target: "src/Api/Endpoints/Orders.cs#Orders.Get", kind: "main", detail: "static Main", declared: false }],
  uses: [],
  files: [],
  key_symbols: [{ id: "Shop.Api.OrderService", anchor: null, path: "src/Api/Orders/OrderService.cs", lines: [1, 40], kind: "type", snippet: "" }],
  inbound: [{ component: "arch/web", exact: 178, inferred: 0 }],
  outbound: [
    { component: "arch/db", exact: 12, inferred: 1 },
    { component: "arch/billing", exact: 3, inferred: 0 },
  ],
  tasks: [],
  docs: [],
  truncated: false,
  folders: [
    { folder: "src/Api/Endpoints", files: 65 },
    { folder: "src/Api/Auth", files: 6 },
    { folder: "src/Api/Tools", files: 2 },
  ],
  folder_edges: [{ from: "src/Api/Endpoints", to: "src/Api/Auth", exact: 47, inferred: 0 }],
};

const DOMAIN: DomainModel = {
  entities: [
    { name: "Order", symbol: "Shop.Domain.Order", file: "src/Domain/Order.cs", line: 3, fields: [{ name: "Id", type: "Guid" }, { name: "Total" }], more_fields: 2 },
    { name: "Line", symbol: "Shop.Domain.Line", fields: [] },
    { name: "Coupon", symbol: "Shop.Domain.Coupon", fields: [] },
  ],
  relations: [{ from: "Order", to: "Line", label: "has many" }],
};

const LLD: Lld = {
  container: HLD.containers[0]!,
  docs: [
    { id: "docs/lld/order-flow", title: "Backend flow: place an order (LLD)", kind: "flow", summary: "How an order is placed.", start: "src/Api/Endpoints/Orders.cs#Orders.Create" },
    { id: "docs/lld/auth", title: "Auth", kind: "component" },
  ],
};

const MODULES: Module[] = [{ id: "mod:api/src/Api/Auth/0", project: "api", label: "src/Api/Auth", files: ["src/Api/Auth/Token.cs"] }];

function routes(over: Record<string, Reply> = {}): Record<string, Reply> {
  return {
    "/api/arch/hld": { body: HLD },
    "/api/arch/graph": {
      body: {
        ...GRAPH,
        drift: [{ kind: "code", from: "arch/api", to: "arch/billing", path: "stone/arch/api.md", line: 5, rule: "arch-drift", message: "m" }],
      },
    },
    "/api/items": { body: [listRow("tasks/CS-1", { status: "in-progress", services: ["API"] }), listRow("tasks/CS-2", { status: "done", services: ["API"] })] },
    "/api/code/explain": { body: EXPLAIN },
    "/api/code/modules": { body: MODULES },
    "/api/code/domain": { body: DOMAIN },
    "/api/arch/lld/arch/api": { body: LLD },
    "/api/timeline": { body: [{ at: "2026-11-01T10:00:00Z", kind: "commit", id: "12f8645", title: "add one-pass export", sha: "12f8645aa" }] },
    ...over,
  };
}

describe("service page", () => {
  it("opens_on_inside_with_key_facts_and_moves_by_tab", async () => {
    fakeApi(routes());
    const router = renderApp("/architecture/arch/api");
    expect(await screen.findByRole("heading", { level: 1, name: "API" })).toBeInTheDocument();
    expect(screen.getByText("Takes orders")).toBeInTheDocument();
    const facts = screen.getByRole("group", { name: "Key facts" });
    await within(facts).findByText("Web");
    const tile = (label: string) => within(facts).getByText(label).parentElement;
    expect(tile("Called by")).toHaveTextContent("Web");
    expect(tile("Uses most")).toHaveTextContent("Store, 13 calls");
    expect(tile("Stack")).toHaveTextContent("ASP.NET");
    await waitFor(() => expect(tile("Running")).toHaveTextContent("1 task"));

    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Inside", "Domain", "Flows", "Commits"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    await screen.findByRole("region", { name: "Inside" });
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual(["Inside", "Connections"]);
    expect(screen.queryByRole("navigation", { name: "On this page" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Domain" }));
    expect(router.state.location.search).toEqual({ tab: "domain" });
    expect(await screen.findByRole("region", { name: "Domain model" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Inside" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Domain" })).toHaveAttribute("aria-selected", "true");
  });

  it("links the parent in the crumbs even when the address does not hold it", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/api");
    const trail = await screen.findByRole("navigation", { name: "Breadcrumb" });
    await waitFor(() => expect(within(trail).getByRole("link", { name: "Shop" })).toHaveAttribute("href", "/architecture/arch/shop"));
    expect(within(trail).getByRole("link", { name: "System architecture" })).toHaveAttribute("href", "/architecture");
    expect(trail).toHaveTextContent("API");
  });

  it("an unknown tab opens Inside", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/api?tab=nonsense");
    expect(await screen.findByRole("region", { name: "Inside" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Inside" })).toHaveAttribute("aria-selected", "true");
  });

  it("counts connections in the code and flags what the docs do not say, and the reverse", async () => {
    fakeApi(routes({ "/api/code/explain": { body: { ...EXPLAIN, outbound: [{ component: "arch/billing", exact: 3, inferred: 0 }] } } }));
    renderApp("/architecture/arch/api");
    const section = await screen.findByRole("region", { name: "Connections" });
    await within(section).findByText(/178 calls found/);
    const billing = within(section).getByRole("link", { name: "billing" }).closest("li");
    expect(billing).toHaveTextContent("Not in the docs");
    const store = within(section).getByRole("link", { name: "Store" }).closest("li");
    expect(store).toHaveTextContent("Not found in the code");
    expect(store).toHaveTextContent("keeps orders");
  });

  it("flags nothing while the code cannot be read, and says why inside", async () => {
    fakeApi(routes({ "/api/code/explain": { status: 422, body: { error: "arch/api has no code yet; add a code list.", kind: "rejected" } } }));
    renderApp("/architecture/arch/api");
    const inside = await screen.findByRole("region", { name: "Inside" });
    expect(await within(inside).findByText(/has no code yet/)).toBeInTheDocument();
    const section = screen.getByRole("region", { name: "Connections" });
    expect(within(section).getByRole("link", { name: "Store" })).toBeInTheDocument();
    expect(section).not.toHaveTextContent("Not found in the code");
  });

  it("groups the modules and says what the chosen one calls and is called by", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/api");
    const inside = await screen.findByRole("region", { name: "Inside" });
    const endpoints = await within(inside).findByRole("button", { name: /Endpoints/ });
    expect(endpoints).toHaveAttribute("aria-pressed", "true");
    expect(within(inside).getByText("Entry points")).toBeInTheDocument();
    expect(within(inside).getByText("Shared inside API")).toBeInTheDocument();
    const selected = () => within(inside).getByRole("complementary", { name: "Selected module" });
    expect(selected()).toHaveTextContent("Entry points, 65 files");
    expect(within(selected()).getByText("Calls").nextElementSibling).toHaveTextContent("Auth 47");

    await userEvent.click(within(inside).getByRole("button", { name: /Auth/ }));
    const chosen = selected();
    expect(within(chosen).getByRole("heading", { name: "Auth" })).toBeInTheDocument();
    expect(within(chosen).getByText("Called by").nextElementSibling).toHaveTextContent("Endpoints 47");
    expect(await within(chosen).findByRole("link", { name: "Open module" })).toHaveAttribute("href", "/architecture/module?id=mod%3Aapi%2Fsrc%2FApi%2FAuth%2F0");
  });

  it("tells apart the modules of one folder by their size", async () => {
    const files = (count: number) => Array.from({ length: count }, (_, at) => `src/Api/Auth/F${at}.cs`);
    fakeApi(
      routes({
        "/api/code/modules": {
          body: [
            { id: "mod:api/src/Api/Auth/0", project: "api", label: "src/Api/Auth", files: files(9) },
            { id: "mod:api/src/Api/Auth/1", project: "api", label: "src/Api/Auth", files: files(2) },
          ],
        },
      }),
    );
    renderApp("/architecture/arch/api");
    const section = await screen.findByRole("region", { name: "Inside" });
    await userEvent.click(await within(section).findByRole("button", { name: /^Auth/ }));
    expect(await within(section).findByRole("link", { name: "Auth, 9 files" })).toHaveAttribute("href", "/architecture/module?id=mod%3Aapi%2Fsrc%2FApi%2FAuth%2F0");
    expect(within(section).getByRole("link", { name: "Auth, 2 files" })).toBeInTheDocument();
  });

  it("draws related entities and lists all of them", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/api?tab=domain");
    const section = await screen.findByRole("region", { name: "Domain model" });
    const diagram = await within(section).findByRole("img", { name: "Domain model diagram" });
    expect(diagram).toHaveTextContent("Order");
    expect(diagram).not.toHaveTextContent("Coupon");
    const list = within(section).getByRole("list", { name: "Entities" });
    expect(list).toHaveTextContent("Coupon");
    expect(list).toHaveTextContent("Id: Guid, Total, and 2 more");
    expect(within(list).getByRole("link", { name: "Order" })).toHaveAttribute("href", `/architecture/class?target=${encodeURIComponent("Shop.Domain.Order")}`);
    expect(list).toHaveTextContent("has many Line");
  });

  it("puts written intent next to code facts and flags where they differ, code first", async () => {
    const listed = { kind: "uses" as const, from: "arch/api", to: "arch/db", path: "stone/arch/api.md", line: 9, rule: "arch-drift", message: "m" };
    fakeApi(
      routes({
        "/api/arch/graph": {
          body: {
            ...GRAPH,
            // The docs row comes first, so only the sort in `disagreements` puts the code row on top.
            drift: [listed, { kind: "code", from: "arch/api", to: "arch/billing", path: "stone/arch/api.md", line: 5, rule: "arch-drift", message: "m" }],
          },
        },
      }),
    );
    renderApp("/architecture/arch/api?tab=flows");
    const section = await screen.findByRole("region", { name: "Flows and design" });
    const written = within(section).getByRole("group", { name: "Written in the docs" });
    expect(await within(written).findByRole("link", { name: "Place an order" })).toHaveAttribute("href", "/docs/lld/order-flow");
    const live = within(section).getByRole("group", { name: "Live from the code" });
    expect(await within(live).findByText("static Main")).toBeInTheDocument();

    const rows = within(await within(section).findByRole("list", { name: "Disagreements" })).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("In the code, not in the docs");
    expect(rows[0]).toHaveTextContent("API calls billing");
    expect(rows[1]).toHaveTextContent("In the docs, not in the code");
    expect(rows[1]).toHaveTextContent("as a dependency");
    expect(screen.getByRole("link", { name: /2 differences with the docs/ })).toHaveAttribute("href", "/architecture/arch/api?tab=flows");
  });

  it("asks for this service's own commits", async () => {
    const { calls } = fakeApi(routes());
    renderApp("/architecture/arch/api?tab=commits");
    const section = await screen.findByRole("region", { name: "Recent commits" });
    expect(await within(section).findByText("add one-pass export")).toBeInTheDocument();
    expect(section).toHaveTextContent(/today|yesterday|ago/);
    expect(section).not.toHaveTextContent("NaN");
    expect(calls.some((url) => url.startsWith("/api/timeline?") && url.includes("component=arch%2Fapi") && url.includes("kind=commit"))).toBe(true);
  });

  it("shows the project's model when a service keeps no entities of its own, and out-of-date links once", async () => {
    const stale = "native edges are out of date for 66 files (portal); run `stone code native` to refresh them";
    fakeApi(
      routes({
        "/api/arch/graph": { body: GRAPH },
        "/api/code/explain": { body: { ...EXPLAIN, note: stale } },
        "/api/code/domain?part=arch%2Fapi": { body: { entities: [], relations: [] } },
        "/api/code/domain": { body: DOMAIN },
      }),
    );
    renderApp("/architecture/arch/api?tab=domain");
    expect(await screen.findByText(/This is the model of the whole project/)).toBeInTheDocument();
    expect(await screen.findAllByText(/is out of date, so some links may be missing/)).toHaveLength(1);
    expect(screen.queryByText(/stone code native/)).toBeNull();
  });

  it("shows an honest line on every tab of a service with no data", async () => {
    fakeApi(
      routes({
        "/api/arch/graph": { body: GRAPH },
        "/api/code/explain": { body: { ...EXPLAIN, inbound: [], outbound: [], folders: [], folder_edges: [], entries: [], key_symbols: [] } },
        "/api/code/domain": { body: { entities: [], relations: [] } },
        "/api/arch/lld/arch/db": { status: 422, body: { error: "no", kind: "rejected" } },
        "/api/timeline": { body: [] },
      }),
    );
    renderApp("/architecture/arch/db");
    expect(await screen.findByText(/found no folders inside/)).toBeInTheDocument();
    expect(screen.getByText("No description written yet.")).toBeInTheDocument();
    const lines: [string, RegExp][] = [
      ["Domain", /No entities found/],
      ["Flows", /Nothing written yet/],
      ["Commits", /No commits found/],
    ];
    for (const [tab, line] of lines) {
      await userEvent.click(screen.getByRole("tab", { name: tab }));
      expect(await screen.findByText(line)).toBeInTheDocument();
    }
  });

  it("a system lists its services", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/shop");
    const list = await screen.findByRole("list", { name: "Services" });
    expect(within(list).getByRole("link", { name: "Web" })).toHaveAttribute("href", "/architecture/arch/web");
  });

  it("says so when no such service exists", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/nothing");
    expect(await screen.findByText("No service called arch/nothing.")).toBeInTheDocument();
  });

  it("offers no write control", async () => {
    fakeApi(routes());
    renderApp("/architecture/arch/api");
    await screen.findByRole("heading", { level: 1, name: "API" });
    await screen.findByRole("region", { name: "Inside" });
    expect(screen.queryByRole("button", { name: /new|edit|rename|delete|save/i })).not.toBeInTheDocument();
  });
});

describe("service model", () => {
  const titleOf = (id: string) => id;

  it("counts code on a child element for the written parent, and leaves the service's own children out", () => {
    const rows = connectionRows(
      "calls",
      "arch/api",
      [{ from: "arch/api", to: "arch/core" }],
      [
        { component: "arch/core/orders", exact: 2, inferred: 0 },
        { component: "arch/api/auth", exact: 9, inferred: 0 },
      ],
      titleOf,
    );
    expect(rows).toEqual([{ id: "arch/core/orders", title: "arch/core/orders", calls: 2, inferredOnly: false, state: "agreed" }]);
  });

  it("counts the differences of a service and of what sits inside it, and no other service", () => {
    const row = (from: string, to?: string) => ({ kind: "uses" as const, from, ...(to === undefined ? {} : { to }), path: "docs/x.md", line: 1, rule: "arch-drift", message: `${from}>${to ?? ""}` });
    const graph: ArchGraph = {
      ...GRAPH,
      drift: [row("arch/api", "arch/db"), row("arch/api/auth", "arch/api/core"), row("arch/web", "arch/api/auth"), row("arch/api-gateway", "arch/web"), row("arch/db")],
    };
    expect(driftRows("arch/api", graph).map((entry) => entry.message)).toEqual(["arch/api>arch/db", "arch/api/auth>arch/api/core", "arch/web>arch/api/auth"]);
    expect(driftRows("arch/api", undefined)).toEqual([]);
  });

  it("counts a task as running only when it is in progress and names the service", () => {
    const rows = [
      listRow("tasks/CS-1", { status: "in-progress", services: ["API"] }),
      listRow("tasks/CS-2", { status: "todo", services: ["API"] }),
      listRow("tasks/CS-3", { status: "in-progress", services: ["Web"] }),
      listRow("tasks/CS-4", { status: "in-progress" }),
    ];
    expect(runningOn({ id: "arch/api", title: "API", type: "container" }, rows).map((row) => row.id)).toEqual(["tasks/CS-1"]);
  });

  it("names code no service owns without flagging it", () => {
    const rows = connectionRows("called-by", "arch/api", [], [{ component: "(unowned)", exact: 0, inferred: 2 }], titleOf);
    expect(rows).toEqual([{ id: null, title: "Code outside any service", calls: 2, inferredOnly: true, state: "agreed" }]);
  });

  it("finds the modules of a folder by their files, not a folder that only starts the same", () => {
    const module = (id: string, files: string[]) => ({ id, project: "p", label: "", files });
    const all = [module("small", ["a/b/x"]), module("other", ["a/bc/x"]), module("big", ["a/b/y", "a/b/z"]), module("deep", ["a/b/c/x"])];
    expect(modulesIn(all, "a/b", ["a/b", "a/bc", "a/b/c"]).map((entry) => entry.id)).toEqual(["big", "small"]);
  });

  it("a service's top folder holds only its own entries, and the others read without the shared path", () => {
    const entries = [{ target: "api/Program.cs", kind: "program", detail: "", declared: false }, { target: "api/Routes/Orders.cs#Orders.Map", kind: "main", detail: "", declared: false }];
    const folders = [{ folder: "api", files: 3 }, { folder: "api/Routes", files: 9 }, { folder: "api/Auth", files: 2 }];
    const groups = moduleGroups({ ...EXPLAIN, folders, folder_edges: [], entries }, "Api");
    expect(groups.map((group) => [group.title, group.modules.map((row) => row.name)])).toEqual([
      ["Entry points", ["api", "Routes"]],
      ["On their own", ["Auth"]],
    ]);
  });

  it("a lone folder reads as its last word", () => {
    const groups = moduleGroups({ ...EXPLAIN, folders: [{ folder: "a/b/Only", files: 1 }], folder_edges: [], entries: [] }, "X");
    expect(groups).toEqual([{ title: "On their own", modules: [expect.objectContaining({ name: "Only" })] }]);
  });

  it("lifts code edges to the services at their ends and sums them", () => {
    const node = (id: string, parent: string | null, type = "component") => ({ id, short: null, path: "", type, title: id, status: null, parent, code: [] });
    const graph: ArchGraph = {
      ...GRAPH,
      nodes: [node("arch/s", null, "system"), node("arch/a", "arch/s"), node("arch/b", "arch/s"), node("arch/a/x", "arch/a"), node("arch/a/y", "arch/a")],
      code_edges: [
        { from: "arch/a/x", to: "arch/b", edges: 2, first: ["a", 1], inferred_only: false },
        { from: "arch/a/y", to: "arch/b", edges: 3, first: ["a", 1], inferred_only: false },
        { from: "arch/a/x", to: "arch/a/y", edges: 9, first: ["a", 1], inferred_only: false },
      ],
    };
    const services = servicesOf(undefined, graph);
    expect(services.map((part) => part.id)).toEqual(["arch/a", "arch/b"]);
    expect(codeConnections(services, graph, [{ from: "arch/b", to: "arch/a", description: "written" }])).toEqual([
      { from: "arch/a", to: "arch/b", description: "5 calls" },
      { from: "arch/b", to: "arch/a", description: "written" },
    ]);
  });
});
