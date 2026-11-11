import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Hld } from "@/api/types/Hld";
import { installFlowMocks } from "@/components/graph/flowMocks";
import { fakeApi, type Reply } from "@/test/fakeApi";
import { listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";

beforeEach(() => installFlowMocks());
afterEach(() => vi.unstubAllGlobals());

const CONTEXT = `C4Context
  title System context diagram for Shop
  Person(buyer, "Buyer", "Orders things", $link="arch/buyer")
  System(shop, "Shop", "Sells things", $link="arch/shop")
  System_Ext(pay, "Payments", "Takes the money", $link="arch/pay")
  Rel(buyer, shop, "Places orders", "HTTPS")
  Rel(shop, pay, "Charges cards", "HTTPS")
`;

const CONTAINERS = `C4Container
  title Container diagram for Shop
  Person(buyer, "Buyer", "Orders things", $link="arch/buyer")
  System_Boundary(shop, "Shop") {
    Container(api, "Api", "Rust", "Serves orders", $link="arch/api")
    ContainerDb(db, "Orders", "PostgreSQL", "Keeps orders", $link="arch/db")
  }
  Rel(buyer, api, "Places orders", "HTTPS")
  Rel(api, db, "Reads and writes", "SQL")
`;

const COMPONENTS = `C4Component
  title Component diagram for Api
  Container(web, "Web portal", "React", "", $link="arch/web")
  Container_Boundary(api, "Api") {
    Component(auth, "Auth", "", "3 files", $link="arch/api/auth")
  }
  Rel(web, auth, "1 reference")
`;

const HLD: Hld = {
  systems: [{ id: "arch/shop", title: "Shop", type: "system" }],
  containers: [
    { id: "arch/api", title: "Api", type: "container", parent: "arch/shop" },
    { id: "arch/web", title: "Web portal", type: "container", parent: "arch/shop" },
  ],
  connections: [],
  drawn: [],
  docs: [],
};

const ITEMS = ["arch/buyer", "arch/shop", "arch/pay", "arch/api", "arch/db", "arch/web", "arch/api/auth", "arch/other"].map((id) => listRow(id));

function routes(over: Record<string, Reply> = {}): Record<string, Reply> {
  return {
    "/api/items": { body: ITEMS },
    "/api/arch/hld": { body: HLD },
    "/api/arch/c4/context": { body: { source: CONTEXT } },
    "/api/arch/c4/container": { body: { source: CONTAINERS } },
    "/api/arch/c4/component/arch/api": { body: { source: COMPONENTS } },
    ...over,
  };
}

/** Opens a box's details and returns the address its "Open the inside view" link has, or null when it has none. */
async function drillDown(label: string): Promise<string | null> {
  // Only the figure: the page shell also writes some of these names, and the open panel repeats them after the boxes.
  const figure = await screen.findByRole("figure");
  fireEvent.click((await within(figure).findAllByText(label))[0]!);
  const panel = await screen.findByRole("complementary", { name: `${label} details` });
  // The link to the item itself shows when the resolver is ready; only then is a missing inside link meaningful.
  await within(panel).findByRole("link", { name: /^Open arch\// });
  return within(panel).queryByRole("link", { name: "Open the inside view" })?.getAttribute("href") ?? null;
}

describe("System space", () => {
  it("the system tab draws the context and opens a system, but not a person or an outside system", async () => {
    fakeApi(routes());
    renderApp("/architecture");

    expect(await screen.findByRole("heading", { level: 1, name: "System context diagram for Shop" })).toBeInTheDocument();
    expect(screen.getByText(/^System architecture\. C4 level 1\./)).toBeInTheDocument();
    const tabs = screen.getByRole("navigation", { name: "System views" });
    expect(within(tabs).getByRole("link", { name: "System architecture" })).toHaveAttribute("aria-current", "page");
    expect(within(tabs).getByRole("link", { name: "Solution architecture" })).toHaveAttribute("href", "/architecture?view=solution");
    expect(within(screen.getByRole("region", { name: "Legend" })).getByText("Blue box: the system. Click it to open the next level")).toBeInTheDocument();

    expect(await drillDown("Shop")).toBe("/architecture?view=solution&system=arch%2Fshop");
    expect(await drillDown("Buyer")).toBeNull();
    expect(await drillDown("Payments")).toBeNull();

    // Name, type and description are on the box, and only a box that opens something carries the link icon.
    const figure = screen.getByRole("figure");
    const box = (name: string) => within(figure).getAllByText(name)[0]!.closest<HTMLElement>("[data-kind]")!;
    expect(box("Shop")).toHaveTextContent("[Software System]");
    expect(box("Shop")).toHaveTextContent("Sells things");
    expect(box("Payments")).toHaveTextContent("[Software System, outside]");
    expect(within(box("Shop")).getByLabelText("Opens the next level")).toBeInTheDocument();
    expect(within(box("Buyer")).queryByLabelText("Opens the next level")).not.toBeInTheDocument();
    expect(within(box("Payments")).queryByLabelText("Opens the next level")).not.toBeInTheDocument();
    // No switch that hides the labels of the lines; the layout tests cover where each label sits.
    expect(within(figure).queryByRole("button", { name: /Labels on/ })).not.toBeInTheDocument();
  });

  it("the solution tab draws the containers of the one system, and a container opens its components", async () => {
    const { calls } = fakeApi(routes());
    renderApp("/architecture?view=solution");

    expect(await screen.findByRole("heading", { level: 1, name: "Container diagram for Shop" })).toBeInTheDocument();
    expect(screen.getByText(/^Solution architecture\. C4 level 2\./)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Solution architecture" })).toHaveAttribute("aria-current", "page");
    expect(within(screen.getByRole("region", { name: "Legend" })).getByText("White box: a data store or message bus")).toBeInTheDocument();
    expect(calls.some((url) => url.startsWith("/api/arch/c4/container"))).toBe(true);

    expect(await drillDown("Api")).toBe("/architecture?view=solution&container=arch%2Fapi");
    expect(await drillDown("Buyer")).toBeNull();
  });

  it("the component diagram names where it sits and opens a component's inside view", async () => {
    fakeApi(routes());
    renderApp("/architecture?view=solution&container=arch%2Fapi");

    expect(await screen.findByRole("heading", { level: 1, name: "Component diagram for Api" })).toBeInTheDocument();
    expect(screen.getByText(/^Solution architecture\. C4 level 3\. What Api is made of/)).toBeInTheDocument();
    const path = await screen.findByLabelText("Diagram path");
    expect(await within(path).findByRole("link", { name: "Shop" })).toHaveAttribute("href", "/architecture?view=solution&system=arch%2Fshop");
    expect(path).toHaveTextContent("Shop / Api");
    expect(screen.getByText(/^Blue boxes: components of Api/)).toBeInTheDocument();

    expect(await drillDown("Auth")).toBe("/architecture/arch/api/auth");
    expect(await drillDown("Web portal")).toBe("/architecture?view=solution&container=arch%2Fweb");
  });

  it("several systems and no choice: lists them instead of asking the server to pick", async () => {
    const landscape = CONTEXT.replace("  System_Ext(pay", '  System(other, "Other", "Second product", $link="arch/other")\n  System_Ext(pay');
    const { calls } = fakeApi(routes({ "/api/arch/c4/context": { body: { source: landscape } } }));
    renderApp("/architecture?view=solution");

    const list = await screen.findByRole("region", { name: "Systems" });
    expect(within(list).getByRole("link", { name: "Shop" })).toHaveAttribute("href", "/architecture?view=solution&system=arch%2Fshop");
    expect(within(list).getByRole("link", { name: "Other" })).toHaveAttribute("href", "/architecture?view=solution&system=arch%2Fother");
    expect(within(list).queryByRole("link", { name: "Payments" })).not.toBeInTheDocument();
    expect(calls.some((url) => url.startsWith("/api/arch/c4/container"))).toBe(false);
  });

  it("a store with no system says which command shows how to describe one", async () => {
    const refused = { status: 422, body: { error: "this store has no system to draw; add one with `stone new system --title <name>`", kind: "rejected" } };
    fakeApi(routes({ "/api/arch/c4/context": refused, "/api/arch/c4/container": refused }));
    renderApp("/architecture");
    expect(await screen.findByText(/stone instructions design/)).toBeInTheDocument();
  });
});
