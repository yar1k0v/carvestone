import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { crumbsFor } from "./crumbs";
import { spaceOf } from "./nav";
import { OnThisPage } from "./OnThisPage";

afterEach(() => vi.unstubAllGlobals());

const HLD = {
  systems: [{ id: "arch/shop", title: "Shop", type: "system" }],
  containers: [
    { id: "arch/shop/api", title: "Orders API", type: "container", parent: "arch/shop" },
    { id: "arch/shop/web", title: "Storefront", type: "container", parent: "arch/shop" },
  ],
  connections: [],
  drawn: [],
  docs: [],
};

function site(extra: Parameters<typeof fakeApi>[0] = {}) {
  return fakeApi({ "/api/items": { body: [listRow("tasks/CS-1", { title: "Pay" })] }, "/api/arch/hld": { body: HLD }, ...extra });
}

async function main() {
  return within(await screen.findByRole("navigation", { name: "Main" }));
}

describe("sidebar", () => {
  it("lists_the_containers_of_any_project", async () => {
    site();
    renderApp("/architecture");
    const nav = await main();
    expect(await nav.findByRole("link", { name: "Orders API" })).toHaveAttribute("href", "/architecture/arch/shop/api");
    expect(nav.getByRole("link", { name: "Storefront" })).toBeInTheDocument();
    expect(await screen.findAllByRole("link", { name: "Shop" })).not.toHaveLength(0);
  });

  it("names_no_project_without_a_single_system", async () => {
    site({ "/api/arch/hld": { status: 404, body: { error: "no design", kind: "not_found" } } });
    renderApp("/");
    expect(await screen.findAllByRole("link", { name: "Carvestone" })).not.toHaveLength(0);
  });

  it("now_has_the_five_pages_and_marks_the_open_one", async () => {
    site({ "/api/timeline": { body: [] } });
    renderApp("/activity");
    const nav = await main();
    expect(await nav.findByRole("link", { name: "Shipped" })).toHaveAttribute("aria-current", "page");
    for (const [name, href] of [
      ["Today", "/"],
      ["Milestones", "/milestones"],
      ["Plans", "/plans"],
      ["Board", "/board"],
    ] as const) {
      expect(nav.getByRole("link", { name })).toHaveAttribute("href", href);
    }
    expect(nav.queryByRole("link", { name: "Code" })).toBeNull();
    expect(nav.queryByRole("link", { name: "Timeline" })).toBeNull();
  });

  it("search_button_opens_the_command_dialog", async () => {
    site();
    renderApp("/");
    const [button] = await screen.findAllByRole("button", { name: /Search/ });
    await userEvent.click(button as HTMLElement);
    expect(await screen.findByRole("dialog", { name: "Search" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Search" })).toBeNull());
  });

  it("ctrl_k_opens_the_command_dialog", async () => {
    site();
    renderApp("/product");
    await screen.findByRole("heading", { level: 1, name: "Product" });
    await userEvent.keyboard("{Control>}k{/Control}");
    expect(await screen.findByRole("textbox", { name: "Search items and code" })).toHaveFocus();
  });

  it("phone_drawer_closes_after_choosing_a_page", async () => {
    site({ "/api/timeline": { body: [] } });
    const router = renderApp("/");
    await userEvent.click(await screen.findByRole("button", { name: "Open menu" }));
    const drawer = await screen.findByRole("dialog", { name: "Menu" });
    await userEvent.click(within(drawer).getByRole("link", { name: "Shipped" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/activity"));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Menu" })).toBeNull());
  });
});

describe("library sidebar", () => {
  const doc = (id: string, title: string, group: string, template = false) => ({ id: `docs/${id}`, title, group, minutes: 2, template });
  const DOCS = [doc("a", "Alpha", "lld"), doc("b", "Beta", "lld"), doc("c", "Gamma", "spec"), doc("t", "Blank", "template", true)];

  it("lists the shelves with the counts of readable docs and marks the open family", async () => {
    site({ "/api/docs": { body: DOCS } });
    renderApp("/docs?group=lld");
    const nav = await main();
    const all = await nav.findByRole("link", { name: /^All documents/ });
    await waitFor(() => expect(all).toHaveTextContent("All documents3"));
    expect(all).toHaveAttribute("href", "/docs");
    expect(all).not.toHaveAttribute("aria-current");
    const family = nav.getByRole("link", { name: /^Low-level design/ });
    expect(family).toHaveTextContent("Low-level design2");
    expect(family).toHaveAttribute("href", "/docs?group=lld");
    expect(family).toHaveAttribute("aria-current", "page");
    expect(nav.getByRole("link", { name: /^Specs/ })).toHaveTextContent("Specs1");
    expect(nav.getByRole("link", { name: "Decisions" })).toHaveAttribute("href", "/docs?shelf=decisions");
    expect(nav.getByRole("link", { name: "Plans" })).toHaveAttribute("href", "/docs?shelf=plans");
    expect(nav.getByRole("link", { name: "Decisions" })).not.toHaveAttribute("aria-current");
  });

  it("marks all documents on the bare library and a shelf on its own", async () => {
    site({ "/api/docs": { body: DOCS }, "/api/items?type=decision": { body: [] } });
    renderApp("/docs?shelf=decisions");
    const nav = await main();
    expect(await nav.findByRole("link", { name: "Decisions" })).toHaveAttribute("aria-current", "page");
    expect(nav.getByRole("link", { name: /^All documents/ })).not.toHaveAttribute("aria-current");
  });

  it("lists the family of the doc being read, by title", async () => {
    site({ "/api/docs": { body: [doc("b", "Beta", "lld"), doc("c", "Gamma", "spec"), doc("a", "Alpha", "lld")] }, "/api/items/docs/b": { body: itemView("docs/b") } });
    renderApp("/docs/b");
    const nav = await main();
    const links = await nav.findAllByRole("link", { name: /^(Alpha|Beta)$/ });
    expect(links.map((link) => link.textContent)).toEqual(["Alpha", "Beta"]);
    expect(links[0]).toHaveAttribute("href", "/docs/a");
    expect(links[1]).toHaveAttribute("aria-current", "page");
    expect(nav.getByRole("link", { name: "All documents" })).toHaveAttribute("href", "/docs");
    expect(nav.queryByRole("link", { name: "Gamma" })).toBeNull();
  });

  it("reads no docs outside the library", async () => {
    const { calls } = site({ "/api/docs": { body: DOCS } });
    renderApp("/");
    await main();
    expect(calls.filter((url) => url.startsWith("/api/docs"))).toEqual([]);
  });
});

describe("spaces", () => {
  it("every_page_lands_in_one_space", () => {
    const cases: [string, ReturnType<typeof spaceOf>][] = [
      ["/", "now"],
      ["/board", "now"],
      ["/milestones/M-1", "now"],
      ["/plans", "now"],
      ["/activity", "now"],
      ["/items/tasks/CS-1", "now"],
      ["/search", "now"],
      ["/product", "product"],
      ["/features/payments", "product"],
      ["/architecture", "system"],
      ["/architecture/arch/shop/api", "system"],
      ["/code", "system"],
      ["/docs/guide/start", "library"],
    ];
    for (const [path, space] of cases) expect(spaceOf(path), path).toBe(space);
  });

  it("the_top_bar_marks_the_open_space_and_a_product_page_has_no_sidebar", async () => {
    site({ "/api/product": { body: { overview: null, features: [] } } });
    renderApp("/product");
    const [bar] = await screen.findAllByRole("navigation", { name: "Spaces" });
    expect(within(bar as HTMLElement).getByRole("link", { name: "Product" })).toHaveAttribute("aria-current", "page");
    expect(within(bar as HTMLElement).getByRole("link", { name: "Now" })).toHaveAttribute("href", "/");
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Open menu" })).toBeNull();
  });
});

describe("routes", () => {
  it("root_opens_now", async () => {
    site();
    const router = renderApp("/");
    expect(await screen.findByRole("heading", { level: 1, name: "Now" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/");
  });

  it("timeline_moves_to_activity_with_its_filters", async () => {
    site({ "/api/timeline": { body: [] } });
    const router = renderApp("/timeline?kind=decision");
    await waitFor(() => expect(router.state.location.pathname).toBe("/activity"));
    expect(router.state.location.search).toMatchObject({ kind: "decision" });
  });

  it("bare_code_moves_to_architecture", async () => {
    site({ "/api/arch/graph": { body: { nodes: [], uses: [], code_edges: [], diagrams: [], drift: [] } } });
    const router = renderApp("/code");
    await waitFor(() => expect(router.state.location.pathname).toBe("/architecture"));
  });

  it("features_route_has_a_page", async () => {
    site();
    renderApp("/features/payments");
    expect(await screen.findByRole("heading", { level: 1, name: "payments" })).toBeInTheDocument();
  });
});

describe("read-only", () => {
  it("no_page_offers_a_write", async () => {
    const task = itemView("tasks/CS-1", { type: "task", fields: { title: "Pay", status: "todo" }, body: "## Goal\n\nPay.\n" });
    site({ "/api/items/tasks/CS-1": { body: task } });
    renderApp("/items/tasks/CS-1");
    await screen.findByRole("heading", { level: 1, name: "Pay" });
    expect(screen.queryByRole("button", { name: /^(New|Edit|Save|Delete|Rename)/ })).toBeNull();
  });

  it("item_sections_fill_on_this_page", async () => {
    const task = itemView("tasks/CS-1", {
      type: "task",
      fields: { title: "Pay" },
      body: "## Goal\n\nPay.\n",
      sections: [{ anchor: "goal", title: "Goal", level: 2, start_line: 1, end_line: 3 }],
    });
    site({ "/api/items/tasks/CS-1": { body: task } });
    renderApp("/items/tasks/CS-1");
    const outline = await screen.findByRole("navigation", { name: "On this page" });
    expect(within(outline).getByRole("link", { name: "Goal" })).toHaveAttribute("href", "#goal");
    await userEvent.click(screen.getByRole("tab", { name: "Provenance" }));
    await waitFor(() => expect(screen.queryByRole("navigation", { name: "On this page" })).toBeNull());
  });
});

describe("on this page", () => {
  it("marks_the_last_heading_above_the_read_line", () => {
    render(
      <main>
        <h2 id="one">One</h2>
        <h2 id="two">Two</h2>
        <OnThisPage
          entries={[
            { id: "one", title: "One" },
            { id: "two", title: "Two" },
          ]}
        />
      </main>,
    );
    // jsdom puts every element at the top, so both headings have passed the line.
    expect(screen.getByRole("link", { name: "Two" })).toHaveAttribute("aria-current", "location");
    expect(screen.getByRole("link", { name: "One" })).not.toHaveAttribute("aria-current");
  });
});

describe("breadcrumbs", () => {
  const titles: Record<string, string> = { "arch/shop": "Shop", "arch/shop/api": "Orders API", "tasks/CS-1": "Pay", "M-1": "First release", "docs/guide/start": "Getting started" };
  const titleOf = (id: string) => titles[id];

  it("the_page_is_the_last_crumb_and_no_link", () => {
    expect(crumbsFor("/product", titleOf)).toEqual([{ label: "Product" }]);
    expect(crumbsFor("/board", titleOf)).toEqual([{ label: "Now" }, { label: "Board" }]);
    expect(crumbsFor("/", titleOf)).toEqual([{ label: "Now" }]);
  });

  it("architecture_climbs_by_element", () => {
    expect(crumbsFor("/architecture/arch/shop/api", titleOf)).toEqual([
      { label: "System architecture", href: "/architecture" },
      { label: "Shop", href: "/architecture/arch/shop" },
      { label: "Orders API" },
    ]);
  });

  it("items_and_docs_read_by_title", () => {
    expect(crumbsFor("/milestones/M-1", titleOf)).toEqual([{ label: "Now" }, { label: "Milestones", href: "/milestones" }, { label: "First release" }]);
    expect(crumbsFor("/items/tasks/CS-1", titleOf)).toEqual([{ label: "Now" }, { label: "Pay" }]);
    expect(crumbsFor("/docs/guide/start", titleOf)).toEqual([{ label: "Library", href: "/docs" }, { label: "Getting started" }]);
    expect(crumbsFor("/items/decisions/ADR-1", titleOf)).toEqual([{ label: "Library", href: "/docs" }, { label: "decisions/ADR-1" }]);
  });
});
