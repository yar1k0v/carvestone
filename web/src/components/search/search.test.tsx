import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DocRow } from "@/api/types/DocRow";
import type { Hit } from "@/api/types/Hit";
import type { ListRow } from "@/api/types/ListRow";
import { fakeApi } from "@/test/fakeApi";
import { renderApp } from "@/test/renderApp";
import { groupHits, highlight } from "./hits";

afterEach(() => vi.unstubAllGlobals());

const SECTION: Hit = { id: "docs/prd", anchor: "search", path: "stone/docs/prd.md", lines: [10, 20], kind: "doc", snippet: "Writes stay a minimal diff." };
const SYMBOL: Hit = { id: "core::Item", anchor: null, path: "crates/core/src/item.rs", lines: [4, 9], kind: "symbol", snippet: "pub struct Item" };
const TASK: Hit = { id: "tasks/CS-1", anchor: null, path: "stone/tasks/CS-1.md", lines: [1, 5], kind: "task", snippet: "Build the cart" };
const FEATURE: Hit = { id: "docs/features/cart", anchor: null, path: "stone/docs/features/cart.md", lines: [1, 5], kind: "doc", snippet: "A cart" };
const NESTED: Hit = { id: "orders::Cart.total", anchor: null, path: "src/orders.rs", lines: [3, 8], kind: "symbol", snippet: "fn total" };
const UNKNOWN: Hit = { id: "docs/ghost", anchor: null, path: "stone/docs/ghost.md", lines: [1, 2], kind: "doc", snippet: "boo" };

const ROW_BASE = { path: "", priority: null, etag: "e" };
const ROWS: ListRow[] = [
  { ...ROW_BASE, id: "tasks/CS-1", short: "CS-1", type: "task", status: "todo", title: "Build the cart" },
  { ...ROW_BASE, id: "docs/prd", short: null, type: "doc", status: null, title: "Product requirements" },
];
const DOCS: DocRow[] = [
  { id: "docs/features/cart", title: "Shopping cart", group: "feature", kind: "overview", minutes: 2, template: false },
  { id: "docs/prd", title: "Product requirements", group: "spec", minutes: 3, template: false },
];

function searchCalls(calls: string[]) {
  return calls.filter((url) => url.startsWith("/api/search"));
}

/** Opens the palette with Ctrl+K and types into it. */
async function typeInPalette(text: string) {
  const user = userEvent.setup();
  await screen.findByRole("navigation", { name: "Main" });
  await user.keyboard("{Control>}k{/Control}");
  await user.type(await screen.findByRole("textbox", { name: "Search items and code" }), text);
  return user;
}

function searchApi(hits: Hit[]) {
  return fakeApi({
    "/api/items": { body: ROWS },
    "/api/docs": { body: DOCS },
    "/api/search": { body: { query: "q", hits, truncated: false } },
  });
}

describe("search", () => {
  it("debounce_one_request", async () => {
    const { calls } = searchApi([SECTION]);
    renderApp("/board");
    await typeInPalette("minimal diff");

    expect(searchCalls(calls)).toHaveLength(0);
    await waitFor(() => expect(searchCalls(calls)).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 350));
    expect(searchCalls(calls)).toEqual(["/api/search?q=minimal+diff&budget=400"]);
  });

  it("hit_opens_section", async () => {
    searchApi([SECTION]);
    const router = renderApp("/board");
    const user = await typeInPalette("minimal diff");
    await screen.findByText("Product requirements");

    await user.keyboard("{Enter}");

    await waitFor(() => expect(router.history.location.pathname).toBe("/docs/prd"));
    expect(router.history.location.hash).toBe("#search");
  });

  it("symbol_hit_opens_its_class_page", async () => {
    searchApi([SYMBOL]);
    const router = renderApp("/board");
    const user = await typeInPalette("Item");
    await user.click(await screen.findByText("Item"));

    await waitFor(() => expect(router.history.location.pathname).toBe("/architecture/class"));
    expect(new URLSearchParams(router.history.location.search).get("target")).toBe("core::Item");
  });

  it("groups_hits_with_titles_product_first", () => {
    const groups = groupHits([NESTED, UNKNOWN, TASK, FEATURE, SYMBOL], ROWS, DOCS);
    const shown = groups.map((group) => [group.name, group.hits.map(({ title, sub }) => [title, sub])]);
    expect(shown).toEqual([
      ["Feature", [["Shopping cart", "Features · overview"]]],
      ["Tasks", [["Build the cart", "CS-1 · todo"]]],
      ["Docs", [["docs/ghost", "Doc"]]],
      [
        "Code",
        [
          ["total", "src/orders.rs"],
          ["Item", "crates/core/src/item.rs"],
        ],
      ],
    ]);
  });

  it("palette_enter_opens_the_row_shown_active", async () => {
    // The answer lists the symbol first; the palette shows the task first.
    searchApi([SYMBOL, TASK]);
    const router = renderApp("/board");
    const user = await typeInPalette("cart");
    const palette = within(await screen.findByRole("dialog"));
    await palette.findByRole("heading", { name: "Tasks" });

    await user.keyboard("{ArrowDown}{Enter}");

    await waitFor(() => expect(router.history.location.pathname).toBe("/architecture/class"));
  });

  it("empty_result_shows_query_and_hint", async () => {
    searchApi([]);
    renderApp("/search?q=nothing+here");
    expect(await screen.findByText(/No results for/)).toHaveTextContent("nothing here");
    expect(screen.getByText(/clear the filters/)).toBeInTheDocument();
  });

  it("highlight_marks_query_words", () => {
    expect(highlight("Writes stay a minimal diff.", "minimal Diff")).toEqual([
      { text: "Writes stay a ", match: false },
      { text: "minimal", match: true },
      { text: " ", match: false },
      { text: "diff", match: true },
      { text: ".", match: false },
    ]);
  });
});
