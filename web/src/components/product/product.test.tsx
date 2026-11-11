import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Feature } from "@/api/types/Feature";
import type { FeatureDetail } from "@/api/types/FeatureDetail";
import type { Product } from "@/api/types/Product";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";
import { renderApp } from "@/test/renderApp";
import { audienceOf, featureHref, filterFeatures, wordsOf } from "./model";

afterEach(() => vi.unstubAllGlobals());

/** The first page with text in a file also loads the renderer, which is slow on a busy machine. */
const COLD_START_MS = 20_000;

function feature(id: string, over: Partial<Feature> = {}): Feature {
  return { id, title: id, platforms: [], open_tasks: 0, done_tasks: 0, inferred_open_tasks: 0, services: [], stale: false, ...over };
}

const PAYMENTS = feature("docs/feature-payments", { title: "Payments", status: "Live", platforms: ["Backend", "Web"], open_tasks: 3, done_tasks: 2, services: ["Api", "Web portal"] });
const REFUNDS = feature("docs/feature-refunds", { title: "Refunds", status: "Hidden", platforms: ["Backend"], stale: true });

const PRODUCT: Product = {
  overview: { id: "docs/product-overview", title: "Shop overview", summary: "A shop for small sellers." },
  features: [PAYMENTS, REFUNDS],
};

const OVERVIEW = itemView("docs/product-overview", {
  type: "doc",
  body: "# Shop overview\n\nOne platform serves two segments:\n\n1. **Sellers** — people who list goods and\n   take orders.\n2. **Buyers**: people who pay.\n\n## Channels\n\nWeb only.\n",
});

const EMPTY_LINKS: FeatureDetail["links"] = { docs: [], tasks: [], parts: [], commits: [] };

function detail(over: Partial<FeatureDetail> = {}): FeatureDetail {
  return { ...PAYMENTS, summary: "Take money.", links: EMPTY_LINKS, ...over };
}

const FEATURE_DOC = itemView("docs/feature-payments", {
  type: "doc",
  fields: { title: "Payments" },
  body: "# Payments\n\nStatus: Live · Platforms: Backend · Web\n\n## What it is\n\nTake money for an order.\n\n## Business rules\n\n1. A refund needs a reason. Route `/api/charges` takes the money, set by `Shop:Currency`.\n",
  sections: [
    { anchor: "what-it-is", title: "What it is", level: 2, start_line: 1, end_line: 2 },
    { anchor: "business-rules", title: "Business rules", level: 2, start_line: 3, end_line: 4 },
  ],
});

const API_PART = { id: "arch/api", title: "Api", kind: "container", inferred: true };

const LLD = {
  container: { id: "arch/api", title: "Api", type: "container" },
  docs: [
    { id: "docs/lld/payment-flow", title: "Payment flow", kind: "flow" },
    { id: "docs/lld/stock-flow", title: "Stock count", kind: "flow" },
  ],
};

describe("product page", () => {
  it("reads_the_overview_whom_it_serves_and_the_features", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/product": { body: PRODUCT }, "/api/items/docs/product-overview": { body: OVERVIEW } });
    renderApp("/product");
    expect(await screen.findByRole("heading", { level: 1, name: "Shop overview" })).toBeInTheDocument();
    expect(screen.getByText("A shop for small sellers.")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Who it serves" })).toBeInTheDocument();
    expect(screen.getByText("Sellers")).toBeInTheDocument();
    const row = screen.getByRole("main").querySelector("tbody tr") as HTMLElement;
    expect(within(row).getByRole("link", { name: "Payments" })).toHaveAttribute("href", "/features/feature-payments");
    expect(within(row).getByText("Live")).toBeInTheDocument();
    expect(within(row).getByText("Backend, Web")).toBeInTheDocument();
    expect(within(row).getByText("3")).toBeInTheDocument();
    expect(within(row).getByText("Api, Web portal")).toBeInTheDocument();
  });

  it("filters_the_features", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/product": { body: PRODUCT }, "/api/items/docs/product-overview": { body: OVERVIEW } });
    renderApp("/product");
    await userEvent.type(await screen.findByRole("searchbox", { name: "Filter features" }), "refu");
    const main = within(screen.getByRole("main"));
    expect(main.getByRole("link", { name: "Refunds" })).toBeInTheDocument();
    expect(main.queryByRole("link", { name: "Payments" })).toBeNull();
    await userEvent.type(screen.getByRole("searchbox", { name: "Filter features" }), "zzz");
    expect(main.getByText("No feature matches this filter.")).toBeInTheDocument();
    await userEvent.clear(screen.getByRole("searchbox", { name: "Filter features" }));
    await userEvent.click(main.getByRole("button", { name: "Docs out of date" }));
    expect(main.getByRole("link", { name: "Refunds" })).toBeInTheDocument();
    expect(main.queryByRole("link", { name: "Payments" })).toBeNull();
    await userEvent.click(main.getByRole("button", { name: "Has open work" }));
    expect(main.getByRole("link", { name: "Payments" })).toBeInTheDocument();
    expect(main.queryByRole("link", { name: "Refunds" })).toBeNull();
    await userEvent.click(main.getByRole("button", { name: "All" }));
    expect(main.getAllByRole("link", { name: /Payments|Refunds/ })).toHaveLength(2);
  });

  it("says_what_is_missing_on_a_project_with_no_product_docs", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/product": { body: { overview: null, features: [] } } });
    renderApp("/product");
    expect(await screen.findByText(/No product overview yet/)).toBeInTheDocument();
    expect(screen.getByText(/No features yet/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Who it serves" })).toBeNull();
  });
});

describe("feature page", () => {
  const routes = (over: Partial<FeatureDetail>, extra = {}) => ({
    "/api/items": { body: [listRow("docs/feature-payments", { title: "Payments" })] },
    "/api/items/docs/feature-payments": { body: FEATURE_DOC },
    "/api/features/docs/feature-payments": { body: detail(over) },
    ...extra,
  });

  it("reads_the_doc_then_technical_then_work", async () => {
    fakeApi(
      routes(
        {
          links: {
            docs: [{ id: "decisions/D-1", title: "Use one currency", type: "decision" }],
            parts: [API_PART],
            tasks: [
              { id: "CS-2", title: "Shipped", status: "done", inferred: false, closed: "2026-10-01", closed_estimated: true },
              { id: "CS-1", title: "Add cards", status: "todo", inferred: true, via: "a tag", closed_estimated: false },
            ],
            commits: [{ sha: "abcdef1234", repo: "", date: "2026-10-01T10:00:00Z", title: "feat: cards", how: "path", task: "CS-1", inferred: true }],
          },
        },
        {
          "/api/arch/lld/arch/api": { body: LLD },
        },
      ),
    );
    renderApp("/features/feature-payments");
    expect(await screen.findByRole("heading", { level: 1, name: "Payments" }, { timeout: COLD_START_MS })).toBeInTheDocument();
    expect(await screen.findByText("Take money for an order.", {}, { timeout: COLD_START_MS })).toBeInTheDocument();
    // The data lines at the top of the doc are not read out as text.
    expect(screen.queryByText(/Status: Live/)).toBeNull();
    // Only the open tab is on the page.
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(screen.queryByRole("list", { name: "Services" })).toBeNull();
    expect(screen.queryByRole("list", { name: "Tasks" })).toBeNull();
    const facts = within(screen.getByRole("complementary", { name: "Feature facts" }));
    expect(facts.getByText("Live")).toBeInTheDocument();
    expect(facts.getByRole("button", { name: "1 task" })).toBeInTheDocument();
    expect(facts.getByRole("link", { name: "Api" })).toHaveAttribute("href", "/architecture/arch/api");

    await userEvent.click(screen.getByRole("tab", { name: "Technical" }));
    expect(screen.queryByText("Take money for an order.")).toBeNull();
    const services = screen.getByRole("list", { name: "Services" });
    expect(within(services).getByRole("link", { name: "Api" })).toHaveAttribute("href", "/architecture/arch/api");
    expect(within(services).getByText("found from code")).toBeInTheDocument();

    const flows = await screen.findByRole("list", { name: "Flows" });
    expect(within(flows).getByRole("link", { name: "Payment flow" })).toBeInTheDocument();
    expect(within(flows).queryByText("Stock count")).toBeNull();
    expect(within(flows).getByText("matched by name")).toBeInTheDocument();

    expect(screen.queryByRole("list", { name: "Tasks" })).toBeNull();

    await userEvent.click(screen.getByRole("tab", { name: "Decisions and docs" }));
    expect(screen.getByRole("link", { name: "Use one currency" })).toHaveAttribute("href", "/items/decisions/D-1");
    expect(screen.queryByRole("list", { name: "Services" })).toBeNull();

    await userEvent.click(screen.getByRole("tab", { name: "Tasks 1" }));
    const tasks = within(screen.getByRole("list", { name: "Tasks" })).getAllByRole("listitem");
    expect(tasks[0]).toHaveTextContent("Add cards");
    expect(tasks[0]).toHaveTextContent("inferred");
    expect(tasks[1]).not.toHaveTextContent("estimated");
    expect(tasks[1]).not.toHaveTextContent("2026-10-01");
    expect(within(tasks[0] as HTMLElement).getByRole("link", { name: "Add cards" })).toHaveAttribute("href", "/tasks/CS-1");

    expect(screen.queryByRole("link", { name: "Use one currency" })).toBeNull();

    await userEvent.click(screen.getByRole("tab", { name: "History" }));
    const commits = screen.getByRole("list", { name: "Commits" });
    expect(within(commits).getByText("abcdef1")).toBeInTheDocument();
    expect(within(commits).getByText("Same files")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Tasks" })).toBeNull();
  }, COLD_START_MS);

  it("opens_on_the_tab_the_address_names", async () => {
    fakeApi(routes({ links: { ...EMPTY_LINKS, tasks: [{ id: "CS-1", title: "Add cards", status: "todo", inferred: false, closed_estimated: false }] } }));
    renderApp("/features/feature-payments?tab=tasks");
    expect(await screen.findByRole("list", { name: "Tasks" }, { timeout: COLD_START_MS })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Tasks 1" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText("Take money for an order.")).toBeNull();
  }, COLD_START_MS);

  it("names_what_is_missing_when_nothing_is_linked", async () => {
    fakeApi(routes({}));
    renderApp("/features/feature-payments");
    await userEvent.click(await screen.findByRole("tab", { name: "Technical" }));
    expect(await screen.findByText(/No services are tied to this feature yet/)).toBeInTheDocument();
    expect(screen.queryByText(/No tasks are tied to this feature yet/)).toBeNull();
    await userEvent.click(screen.getByRole("tab", { name: "Tasks 0" }));
    expect(screen.getByText(/No tasks are tied to this feature yet/)).toBeInTheDocument();
    expect(screen.queryByText(/No services are tied to this feature yet/)).toBeNull();
    await userEvent.click(screen.getByRole("tab", { name: "History" }));
    expect(screen.getByText(/No commits yet/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Decisions and docs" }));
    expect(screen.getByText(/No other doc or decision/)).toBeInTheDocument();
    expect(screen.queryByText(/No commits yet/)).toBeNull();
  });

  it("points_to_the_service_when_nothing_is_tied_to_the_feature", async () => {
    fakeApi(
      routes(
        { title: "Zzzz", links: { ...EMPTY_LINKS, parts: [API_PART] } },
        {
          "/api/items/docs/feature-payments": { body: itemView("docs/feature-payments", { type: "doc", body: "Nothing to match.\n" }) },
          "/api/arch/lld/arch/api": { body: LLD },
        },
      ),
    );
    renderApp("/features/feature-payments");
    await userEvent.click(await screen.findByRole("tab", { name: "Technical" }));
    const flows = (await screen.findByText(/No flows are tied to this feature yet/)) as HTMLElement;
    expect(within(flows).getByRole("link", { name: "Api (2)" })).toHaveAttribute("href", "/architecture/arch/api?tab=flows");
  });

  it("is_read_only", async () => {
    const { fetchMock } = fakeApi(routes({}));
    renderApp("/features/feature-payments");
    await screen.findByText("Take money for an order.");
    expect(fetchMock.mock.calls.every(([, init]) => (init?.method ?? "GET") === "GET")).toBe(true);
  });
});

describe("product model", () => {
  it("feature_links_drop_the_docs_folder", () => {
    expect(featureHref("docs/feature/fuel")).toBe("/features/feature/fuel");
  });

  it("audience_comes_from_a_section_first", () => {
    expect(audienceOf("Intro.\n\n## Who it serves\n\n- **Owners**: run the shop.\n- Clerks\n\n## Next\n\n- **Not this**\n")).toEqual([
      { name: "Owners", text: "run the shop." },
      { name: "Clerks", text: "" },
    ]);
  });

  it("audience_joins_wrapped_lines_of_an_opening_list", () => {
    expect(audienceOf("# T\n\nIt serves:\n\n1. **A** — one\n   more\n2. **B** — two\n\nThen text.\n- **Not this**\n")).toEqual([
      { name: "A", text: "one more" },
      { name: "B", text: "two" },
    ]);
  });

  it("no_audience_when_the_doc_does_not_say", () => {
    expect(audienceOf("# T\n\nA tool.\n\n- one\n- two\n")).toEqual([]);
  });

  it("filter_reads_status_and_platforms", () => {
    expect(filterFeatures([PAYMENTS, REFUNDS], "hidden")).toEqual([REFUNDS]);
    expect(filterFeatures([PAYMENTS, REFUNDS], " web ")).toEqual([PAYMENTS]);
  });

  it("words_split_code_names_and_ignore_plurals", () => {
    expect(wordsOf("Geofence Rules")).toEqual(wordsOf("GeofenceRule"));
    expect([...wordsOf("Addresses and Entries")]).toEqual(["address", "entry"]);
  });
});
