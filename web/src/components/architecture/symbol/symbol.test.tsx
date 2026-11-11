import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Explain } from "@/api/types/Explain";
import type { Hit } from "@/api/types/Hit";
import type { SymbolPage } from "@/api/types/SymbolPage";
import type { Why } from "@/api/types/Why";
import { installFlowMocks } from "@/components/graph/flowMocks";
import { type Reply, fakeApi } from "@/test/fakeApi";
import { renderApp } from "@/test/renderApp";
import { classHref, moduleHref } from "./href";
import { folderName, kindWord, nameOf, ownerOf, sourceLines, uniqueMembers } from "./model";

beforeEach(() => installFlowMocks());
afterEach(() => vi.unstubAllGlobals());

const CLASS = "Shop.Orders.OrderService";
const PATH = "src/Orders/OrderService.cs";
const MODULE = "mod:Shop/src/Orders/1";

function hit(id: string, over: Partial<Hit> = {}): Hit {
  return { id, anchor: null, path: PATH, lines: [10, 24], kind: "method", snippet: `void ${id}()`, exactness: "exact", ...over };
}

const WHY: Why = {
  path: PATH,
  line: 5,
  component: "arch/api",
  commits: [{ sha: "abcdef123456", date: "2026-04-24T02:51:19+03:00", author: "A", subject: "add orders", uncommitted: false }],
  tasks: [],
  plans: [],
  decisions: [],
  docs: [{ id: "docs/orders", status: null, title: "Orders", via: "component", commits: 0 }],
  fallback: "component",
  shallow: false,
  truncated: false,
};

const BASE: Record<string, Reply> = {
  "/api/items": { body: [] },
  "/api/why": { body: WHY },
  "/api/code/modules": { body: [{ id: MODULE, project: "Shop", label: "src/Orders", files: [PATH] }] },
};

const PAGE: SymbolPage = {
  id: CLASS,
  name: "OrderService",
  kind: "type",
  signature: "public sealed class OrderService",
  doc: "Takes an order from the cart to the store.",
  source: { file: PATH, start_line: 5, end_line: 7, text: "public sealed class OrderService\r\n{\r\n}\r\n", truncated: false },
  source_private: false,
  members: [
    {
      id: `${CLASS}.Place`,
      name: "Place",
      kind: "method",
      signature: "public void Place()",
      short: "Place()",
      lines: [10, 24],
      depth: 0,
      doc: "Saves the order.",
      used_by: 2,
      uses: 1,
    },
  ],
  used_by: [{ hit: hit("Shop.Api.OrdersEndpoint.Post", { kind: "calls", path: "src/Api/OrdersEndpoint.cs" }), through: ["Place"] }],
  uses: [{ hit: hit("Shop.Data.OrderRepository.Save", { kind: "calls", exactness: "inferred" }), through: [] }],
};

function symbol(target: string): string {
  return `/api/code/symbol?${new URLSearchParams({ target }).toString()}`;
}

function classApi(extra: Record<string, Reply> = {}) {
  return fakeApi({
    ...BASE,
    [`/api/items/${CLASS}`]: { body: { id: CLASS, path: PATH, type: "symbol", lines: [5, 40], backlinks: [] } },
    [symbol(CLASS)]: { body: PAGE },
    ...extra,
  });
}

describe("class page", () => {
  it("reads_top_to_bottom: purpose, members, used by, uses and source, from one request", async () => {
    const { calls } = classApi();
    renderApp(classHref(CLASS));

    expect(await screen.findByRole("heading", { level: 1, name: "OrderService" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual([
      "Purpose",
      "Members",
      "Used by",
      "Uses",
      "Source",
    ]);
    expect(screen.getByText("Takes an order from the cart to the store.")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Purpose" })).getByText("public sealed class OrderService")).toBeInTheDocument();

    const members = screen.getByRole("region", { name: "Members" });
    expect(within(members).getByRole("link", { name: "Place" })).toHaveAttribute("href", classHref(`${CLASS}.Place`));
    expect(within(members).getByText("Place()")).toBeInTheDocument();
    expect(within(members).getByText("Saves the order.")).toBeInTheDocument();
    expect(within(members).getByText("used by 2, uses 1")).toBeInTheDocument();

    const usedBy = screen.getByRole("region", { name: "Used by" });
    expect(within(usedBy).getByRole("link", { name: "Shop.Api.OrdersEndpoint.Post" })).toBeInTheDocument();
    expect(within(usedBy).getByText("through Place")).toBeInTheDocument();

    const uses = screen.getByRole("region", { name: "Uses" });
    expect(within(uses).getByRole("link", { name: "Shop.Data.OrderRepository.Save" })).toBeInTheDocument();
    expect(within(uses).getByText("inferred")).toBeInTheDocument();

    const source = screen.getByRole("region", { name: "Source" });
    expect(within(source).getByText(":5-7", { exact: false })).toBeInTheDocument();
    expect(within(within(source).getByRole("group", { name: "Source code" })).getAllByRole("row")).toHaveLength(3);
    expect(await within(source).findByText("add orders")).toBeInTheDocument();
    expect(calls.filter((url) => url.startsWith("/api/code/symbol"))).toHaveLength(1);
    expect(calls.filter((url) => /\/api\/code\/(uses|deps|outline|find)/.test(url))).toEqual([]);
  });

  it("links_up_the_ladder: the purpose names its module and service and says how a doc relates", async () => {
    classApi();
    renderApp(classHref(CLASS));
    const purpose = await screen.findByRole("region", { name: "Purpose" });
    await within(purpose).findByText("written for its service");
    expect(within(purpose).getAllByRole("link", { name: "Orders" }).map((link) => link.getAttribute("href"))).toEqual([
      moduleHref(MODULE),
      "/docs/orders",
    ]);
    expect(within(purpose).getByRole("link", { name: "api" })).toHaveAttribute("href", "/architecture/arch/api");
  });

  it("ambiguous_name: the candidates show and choosing one opens it", async () => {
    classApi({
      [symbol("Run")]: {
        status: 422,
        body: { error: `Run matches 2 symbols; use one of: Shop.A.Run, ${CLASS}`, kind: "invalid" },
      },
    });
    const router = renderApp(classHref("Run"));
    const list = await screen.findByRole("region", { name: "Candidates" });
    await userEvent.click(within(list).getByRole("button", { name: CLASS }));
    expect(await screen.findByRole("heading", { level: 1, name: "OrderService" })).toBeInTheDocument();
    expect(router.state.location.search).toMatchObject({ target: CLASS });
  });

  it("short_name_settles: a page opened by a short name shows the name the engine resolved", async () => {
    classApi({ [symbol("OrderService")]: { body: PAGE } });
    renderApp(classHref("OrderService"));
    expect(await screen.findByText(CLASS)).toBeInTheDocument();
  });

  it("file_target: a file reads by its file name and shows its text", async () => {
    fakeApi({
      ...BASE,
      [`/api/items/${PATH}`]: { body: { id: PATH, path: PATH, type: "file", lines: [1, 40], backlinks: [] } },
      [symbol(PATH)]: {
        body: {
          ...PAGE,
          id: PATH,
          name: "OrderService",
          kind: "file",
          signature: "",
          doc: undefined,
          source: { file: PATH, start_line: 1, end_line: 1, text: "namespace Shop;\n", truncated: false },
        },
      },
    });
    renderApp(classHref(PATH));
    expect(await screen.findByRole("heading", { level: 1, name: "OrderService.cs" })).toBeInTheDocument();
    expect(screen.getByText("namespace Shop;")).toBeInTheDocument();
  });

  it("private_source: a configuration file says why its text is not shown", async () => {
    const hidden = { ...PAGE, source_private: true };
    delete hidden.source;
    classApi({ [symbol(CLASS)]: { body: hidden } });
    renderApp(`/architecture/class?target=${encodeURIComponent(CLASS)}`);
    expect(await screen.findByText(/the index has no language for this file/)).toBeInTheDocument();
  });

  it("long_source: a cut text says where the symbol really ends", async () => {
    classApi({ [symbol(CLASS)]: { body: { ...PAGE, source: { ...PAGE.source!, end_line: 9000, truncated: true } } } });
    renderApp(classHref(CLASS));
    expect(await screen.findByText("Too long to show in full: it goes on to line 9000.")).toBeInTheDocument();
  });

  it("no_source: a file gone from disk leaves the rest of the page standing", async () => {
    classApi({ [symbol(CLASS)]: { body: { ...PAGE, source: undefined, members: [], used_by: [], uses: [] } } });
    renderApp(classHref(CLASS));
    expect(await screen.findByText("The file could not be read.")).toBeInTheDocument();
    expect(screen.getByText("Nothing is declared inside it.")).toBeInTheDocument();
    expect(screen.getByText("Nothing in the code uses this.")).toBeInTheDocument();
  });

  it("old_code_link_opens_the_class_page", async () => {
    classApi();
    const router = renderApp(`/code?target=${encodeURIComponent(CLASS)}`);
    await waitFor(() => expect(router.state.location.pathname).toBe("/architecture/class"));
    expect(router.state.location.search).toMatchObject({ target: CLASS });
  });
});

const EXPLAIN: Explain = {
  target: `${MODULE} (src/Orders)`,
  entries: [{ target: `${PATH}#${CLASS}`, kind: "main", detail: "static Main", declared: false }],
  uses: [],
  files: [hit(PATH, { kind: "file", lines: [1, 40], snippet: "csharp", exactness: undefined })],
  key_symbols: [hit(CLASS, { kind: "type", snippet: "in 5 public sealed class OrderService" })],
  inbound: [
    { component: "mod:Shop/src/Api/1", exact: 1, inferred: 0 },
    { component: "mod:Shop/src/Jobs/1", exact: 3, inferred: 2 },
  ],
  outbound: [{ component: "(unowned)", exact: 1, inferred: 0 }],
  tasks: [],
  docs: ["docs/orders"],
  truncated: false,
};

function moduleApi(extra: Record<string, Reply> = {}) {
  return fakeApi({
    ...BASE,
    "/api/code/modules": {
      body: [
        { id: MODULE, project: "Shop", label: "src/Orders", files: [PATH] },
        { id: "mod:Shop/src/Jobs/1", project: "Shop", label: "src/Jobs", files: [] },
      ],
    },
    "/api/code/explain": { body: EXPLAIN },
    ...extra,
  });
}

describe("module page", () => {
  it("reads_top_to_bottom: its symbols, the modules around it and its files link down and across", async () => {
    moduleApi();
    renderApp(moduleHref(MODULE));

    expect(await screen.findByRole("heading", { level: 1, name: "Orders" })).toBeInTheDocument();
    expect(screen.getByText(/1 file in the project Shop/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "static Main" })).toHaveAttribute("href", classHref(CLASS));

    const members = screen.getByRole("region", { name: "Members" });
    expect(within(members).getByRole("link", { name: "OrderService" })).toHaveAttribute("href", classHref(CLASS));
    expect(within(members).getByText("class, 5 incoming links")).toBeInTheDocument();
    expect(within(members).getByText("public sealed class OrderService")).toBeInTheDocument();

    const usedBy = screen.getByRole("region", { name: "Used by" });
    const peers = within(usedBy).getAllByRole("link");
    expect(peers.map((link) => link.textContent)).toEqual(["src/Jobs", "mod:Shop/src/Api/1"]);
    expect(peers[0]).toHaveAttribute("href", moduleHref("mod:Shop/src/Jobs/1"));
    expect(within(usedBy).getByText("5 links, 2 inferred")).toBeInTheDocument();

    const uses = screen.getByRole("region", { name: "Uses" });
    expect(within(uses).getByText("Code outside any module")).toBeInTheDocument();
    expect(within(uses).queryByRole("link")).not.toBeInTheDocument();

    const source = screen.getByRole("region", { name: "Source" });
    expect(within(source).getByRole("link", { name: PATH })).toHaveAttribute("href", classHref(PATH));
    expect(within(source).getByText("csharp, 40 lines")).toBeInTheDocument();
  });

  it("gone_module: an id the code no longer has shows the engine's sentence", async () => {
    moduleApi({
      "/api/code/explain": { status: 422, body: { error: "no module `mod:x`; module ids change when the code does", kind: "invalid" } },
    });
    renderApp(moduleHref("mod:x"));
    expect(await screen.findByRole("alert")).toHaveTextContent("no module `mod:x`");
  });

  it("empty_module: a module with nothing around it reads as such and does not crash", async () => {
    moduleApi({
      "/api/code/explain": { body: { ...EXPLAIN, entries: [], files: [], key_symbols: [], inbound: [], outbound: [], docs: [] } },
    });
    renderApp(moduleHref(MODULE));
    expect(await screen.findByText("No other module uses this one.")).toBeInTheDocument();
    expect(screen.getByText("No document or task is tied to this module yet.")).toBeInTheDocument();
    expect(screen.getByText("The index holds no files for this module.")).toBeInTheDocument();
  });
});

describe("folder page", () => {
  const FOLDER = "src/Orders";
  const FOLDER_EXPLAIN = {
    ...EXPLAIN,
    target: `${FOLDER}/`,
    folders: [
      { folder: FOLDER, files: 2 },
      { folder: `${FOLDER}/Rules`, files: 31 },
    ],
  };

  it("names_services: a service next to the folder reads as its title", async () => {
    moduleApi({
      "/api/code/explain": { body: { ...FOLDER_EXPLAIN, inbound: [{ component: "arch/web", exact: 3, inferred: 0 }] } },
      "/api/arch/hld": { body: { systems: [], containers: [{ id: "arch/web", title: "Web portal", type: "container" }], connections: [], drawn: [], docs: [] } },
    });
    renderApp(moduleHref(FOLDER));
    const usedBy = await screen.findByRole("region", { name: "Used by" });
    expect(await within(usedBy).findByRole("link", { name: "Web portal" })).toHaveAttribute("href", "/architecture/arch/web");
  });

  it("old_address_opens_it: a folder link lands on the module page, and counts all its files", async () => {
    moduleApi({ "/api/code/explain": { body: FOLDER_EXPLAIN } });
    const router = renderApp(`/architecture/${FOLDER}`);
    expect(await screen.findByRole("heading", { level: 1, name: "Orders" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/architecture/module");
    expect(router.state.location.search).toMatchObject({ id: FOLDER });
    expect(screen.getByText("Folder")).toBeInTheDocument();
    expect(screen.getByText(/^33 files/)).toBeInTheDocument();
    expect(screen.getByText("Showing 1 of 33 files.")).toBeInTheDocument();
  });
});

describe("symbol model", () => {
  it("names_split_on_every_language_separator", () => {
    expect(nameOf("a::b::C")).toBe("C");
    expect(nameOf("src/a.cs", true)).toBe("a.cs");
    expect(ownerOf("a::B::c")).toBe("a::B");
    expect(ownerOf("alone")).toBeNull();
  });

  it("kind_word_reads_the_signature", () => {
    expect(kindWord("type", "public interface IClock")).toBe("Interface");
    expect(kindWord("type", "type X = string")).toBe("Type");
    expect(kindWord("member", "int x")).toBe("Field");
  });

  it("source_lines_keep_the_file's_numbers_and_ignore_the_last_line_end", () => {
    expect(sourceLines("a\r\nb\n", 7)).toEqual([
      { number: 7, text: "a" },
      { number: 8, text: "b" },
    ]);
    expect(sourceLines("a", 1)).toEqual([{ number: 1, text: "a" }]);
  });

  it("a_member_found_twice_shows_once_at_its_deepest", () => {
    const [member] = PAGE.members;
    const rows = uniqueMembers([member!, { ...member!, depth: 2 }, { ...member!, id: "other" }]);
    expect(rows.map((row) => [row.id, row.depth])).toEqual([
      [member!.id, 2],
      ["other", 0],
    ]);
  });

  it("folder_name_is_the_last_word", () => {
    expect(folderName("src/Orders/")).toBe("Orders");
    expect(folderName("src")).toBe("src");
  });
});
