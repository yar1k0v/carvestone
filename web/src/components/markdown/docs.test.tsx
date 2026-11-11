import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DocRow } from "@/api/types/DocRow";
import { App } from "@/App";
import { connectionsOf } from "@/components/docs/connections";
import { parseFact, readDoc, shortCode } from "@/components/docs/lead";
import { ago, byDate, filterDocs, groupCounts, rememberDoc } from "@/components/docs/library";
import { matchingLine, plainLine } from "@/components/docs/matchLine";
import { buildResolver } from "@/components/markdown/links";
import { createMemoryRouter, createQueryClient } from "@/router";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => localStorage.clear());

function doc(id: string, over: Partial<DocRow> = {}): DocRow {
  return { id: `docs/${id}`, title: id, group: "other", minutes: 2, template: false, ...over };
}

function isoDaysAgo(days: number): string {
  const day = new Date();
  day.setDate(day.getDate() - days);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
}

describe("library model", () => {
  const NOW = new Date(2026, 10, 4, 15, 0);

  it("reads a date the way a person says it", () => {
    expect(ago("2026-11-04", NOW)).toBe("today");
    expect(ago("2026-11-03", NOW)).toBe("yesterday");
    expect(ago("2026-11-01", NOW)).toBe("3 days ago");
    expect(ago("2026-10-14", NOW)).toBe("3 weeks ago");
    expect(ago("2026-06-01", NOW)).toBe("5 months ago");
  });

  it("puts rows under date headings, leaving empty ones out", () => {
    const groups = byDate(
      [doc("a", { updated: "2026-11-04" }), doc("b", { updated: "2026-11-01" }), doc("c", { updated: "2026-01-01" }), doc("d")],
      NOW,
    );
    expect(groups.map((group) => [group.label, group.rows.length])).toEqual([
      ["Today", 1],
      ["This week", 1],
      ["Older", 2],
    ]);
  });

  it("filters by words in the title or summary, by family, and leaves templates out", () => {
    const rows = [
      doc("map", { title: "Live map", group: "lld" }),
      doc("list", { title: "Vehicle list", group: "lld", summary: "Opens the live view" }),
      doc("guide", { title: "Guide", group: "spec" }),
      doc("blank", { title: "<Component>", group: "template", template: true }),
    ];
    expect(filterDocs(rows, { q: "LIVE" }).map((row) => row.title)).toEqual(["Live map", "Vehicle list"]);
    expect(filterDocs(rows, { q: "live view" }).map((row) => row.title)).toEqual(["Vehicle list"]);
    expect(filterDocs(rows, { group: "spec" }).map((row) => row.title)).toEqual(["Guide"]);
    expect(filterDocs(rows, {})).toHaveLength(3);
    expect(filterDocs(rows, { templates: true })).toHaveLength(4);
  });

  it("counts families largest first with Other last", () => {
    const rows = [doc("a", { group: "other" }), doc("b", { group: "other" }), doc("c", { group: "other" }), doc("d", { group: "lld" })];
    expect(groupCounts(rows)).toEqual([
      { value: "lld", label: "Low-level design", count: 1 },
      { value: "other", label: "Other", count: 3 },
    ]);
  });

  it("keeps a doc found in its text, and only the docs of the chosen part", () => {
    const rows = [doc("a", { title: "Alpha" }), doc("b", { title: "Beta" })];
    expect(filterDocs(rows, { q: "gamma" }, new Map([["docs/b", {}]])).map((row) => row.title)).toEqual(["Beta"]);
    expect(filterDocs(rows, {}, undefined, new Set(["docs/a"])).map((row) => row.title)).toEqual(["Alpha"]);
  });
});

describe("what a doc opens with", () => {
  const BODY =
    "# Login flow (LLD)\n\nKind: flow · Repo: mobile · Arch item: [[arch/mobile]] · Code: `a/b/c`, `d/e`\nRelated: [[docs/x]], [[docs/y]]\n\n## In plain words\n\nA user signs in.\n";

  it("drops a first heading that repeats the title and lifts the data lines out", () => {
    const read = readDoc(BODY, "Login flow (LLD)");
    expect(read.body.startsWith("## In plain words")).toBe(true);
    expect(read.facts.map((fact) => fact.key)).toEqual(["Kind", "Repo", "Arch item", "Code", "Related"]);
    expect(read.facts[3]?.value).toBe("`a/b/c`, `d/e`");
  });

  it("follows a data line that wrapped", () => {
    const read = readDoc("Status: done · Platforms: Web · App (both\nmodes)\n\nText.\n", "T");
    expect(read.facts).toEqual([
      { key: "Status", value: "done" },
      { key: "Platforms", value: "Web · App (both modes)" },
    ]);
    expect(read.body).toBe("Text.\n");
  });

  it("keeps a first heading that says something else", () => {
    expect(readDoc("# Another\n\nText.\n", "Title").body).toBe("# Another\n\nText.\n");
  });

  it("leaves a lone key-like sentence as prose", () => {
    const read = readDoc("Status: implemented in most places.\n\nMore.\n", "T");
    expect(read.facts).toEqual([]);
    expect(read.body).toContain("Status: implemented");
  });

  it("parses a value into links, code and what is left", () => {
    expect(parseFact("[[arch/mobile]], `a/b` and more")).toEqual({
      parts: [
        { kind: "link", target: "arch/mobile" },
        { kind: "code", text: "a/b" },
      ],
      rest: "and more",
    });
  });

  it("shortens code to what a reader needs", () => {
    expect(shortCode("mobile/lib/features/search")).toBe("…/features/search");
    expect(shortCode("mobile/lib/a.dart#SearchViewModel")).toBe("SearchViewModel");
    expect(shortCode("a/b")).toBe("a/b");
  });
});

describe("the matching line", () => {
  const BODY = "Intro with nothing.\n\n## Limits\n\n- The **retry** count is `3`.\n- Retry waits a [minute](x.md) between tries.\n";

  it("finds the line with the most words and the section it sits in", () => {
    expect(matchingLine(BODY, "retry minute")).toEqual({ text: "Retry waits a minute between tries.", anchor: "limits" });
    expect(matchingLine(BODY, "retry nothing-here")).toEqual({ text: "The retry count is 3.", anchor: "limits" });
    expect(matchingLine(BODY, "intro")).toEqual({ text: "Intro with nothing.", anchor: null });
    expect(matchingLine(BODY, "absent")).toBeNull();
  });

  it("cuts a long line around the match, and skips the rule of a table", () => {
    const line = matchingLine(`${"a ".repeat(200)}needle${" b".repeat(200)}`, "needle");
    expect(line?.text.length).toBeLessThan(160);
    expect(line?.text).toContain("needle");
    expect(matchingLine("| --- | --- |\n| a-b | c |\n", "a-b")?.text).toBe("a-b | c");
  });

  it("reads a line as the page does", () => {
    expect(plainLine("### See [[docs/x]] and [the guide](a.md)")).toBe("See docs/x and the guide");
    expect(plainLine("| one | two |")).toBe("one | two");
  });
});

describe("what a doc connects to", () => {
  const rows = [
    listRow("docs/self", { title: "Self" }),
    listRow("docs/other", { title: "Other doc" }),
    listRow("docs/guide/start", { title: "Start" }),
    listRow("tasks/CS-1", { title: "A task", type: "task", short: "CS-1", status: "doing" }),
    listRow("plans/rollout", { title: "Rollout", type: "plan" }),
    listRow("arch/app", { title: "The app", type: "container" }),
  ];
  const base = { self: "docs/self", facts: [], links: [], body: "", backlinks: [], rows, resolver: buildResolver(rows) };

  it("sorts everything named into its group and says how it relates", () => {
    const found = connectionsOf({
      ...base,
      facts: [
        { key: "Arch item", value: "[[arch/app]]" },
        { key: "Related", value: "[[docs/other]], [[CS-1]]" },
        { key: "Depends", value: "[[plans/rollout]]" },
        { key: "Code", value: "`a/b/c/d.ts#Thing`" },
      ],
    });
    expect(found.architecture.map((entry) => [entry.title, entry.relation, entry.href])).toEqual([
      ["The app", "The part this document describes", "/architecture/arch/app"],
      ["Thing", "Code this document covers", "/code?target=Thing"],
    ]);
    expect(found.documents.map((entry) => [entry.title, entry.relation])).toEqual([["Other doc", "Listed as related"]]);
    expect(found.tasks.map((entry) => [entry.short, entry.title, entry.status, entry.relation])).toEqual([
      ["CS-1", "A task", "doing", "Listed as related"],
      [undefined, "Rollout", undefined, 'Named under "Depends"'],
    ]);
  });

  it("leaves routes and permissions out of the code", () => {
    const found = connectionsOf({ ...base, facts: [{ key: "Entry", value: "`a/b.ts#Page` (routes `/vehicles`; permission `vehicle:read`)" }] });
    expect(found.architecture.map((entry) => [entry.title, entry.relation])).toEqual([["Page", "Where the code starts"]]);
  });

  it("reads links in the text but not in code, and joins the reasons of one entry", () => {
    const found = connectionsOf({
      ...base,
      links: ["docs/other"],
      body: "Top [[CS-1]].\n\n## Scope\n\nSee [[docs/other]], [the start](guide/start.md) and [[code:Thing]].\n\nNot `[[arch/app]]`.\n\n```\n[[arch/app]]\n```\n",
      backlinks: [
        { id: "docs/other", anchor: "open-items", path: "stone/docs/other.md", line: 3, kind: "wikilink" },
        { id: "docs/other", anchor: null, path: "stone/docs/other.md", line: 1, kind: "links" },
        { id: "docs/self", anchor: null, path: "stone/docs/self.md", line: 1, kind: "wikilink" },
      ],
    });
    expect(found.tasks.map((entry) => entry.relation)).toEqual(["Mentioned in the text"]);
    expect(found.documents.map((entry) => [entry.title, entry.relation])).toEqual([
      ["Other doc", 'This document links to it · Mentioned in "Scope" · and 2 more'],
      ["Start", 'Mentioned in "Scope"'],
    ]);
    expect(found.architecture.map((entry) => entry.title)).toEqual(["Thing"]);
  });

  it("says who points here, for a doc that names nothing", () => {
    const found = connectionsOf({
      ...base,
      backlinks: [
        { id: "tasks/CS-1", anchor: "open-items", path: "stone/tasks/CS-1.md", line: 3, kind: "wikilink" },
        { id: "arch/app", anchor: null, path: "stone/arch/app.md", line: 1, kind: "diagram" },
      ],
    });
    expect(found.tasks[0]?.relation).toBe('Mentions this document in "Open items"');
    expect(found.architecture[0]?.relation).toBe("Shows this document in a diagram");
  });
});

describe("docs library", () => {
  function open(path: string, rows: DocRow[], extra: Parameters<typeof fakeApi>[0] = {}) {
    const api = fakeApi({ "/api/docs": { body: rows }, "/api/items": { body: [] }, ...extra });
    const router = createMemoryRouter(createQueryClient(), path);
    render(<App router={router} />);
    return { router, calls: api.calls };
  }

  const ROWS = [
    doc("fresh", { title: "Fresh doc", summary: "Says what changed", group: "lld", kind: "flow", updated: isoDaysAgo(0), minutes: 6 }),
    doc("week", { title: "Last week doc", group: "spec", updated: isoDaysAgo(3) }),
    doc("blank", { title: "<Blank>", group: "template", template: true, updated: isoDaysAgo(0) }),
  ];

  it("opens on the newest docs under date headings, with what each says and when", async () => {
    open("/docs", ROWS);
    const today = await screen.findByRole("region", { name: "Today" });
    const link = within(today).getByRole("link", { name: "Fresh doc" });
    expect(link).toHaveAttribute("href", "/docs/fresh");
    expect(today).toHaveTextContent("Says what changed");
    expect(today).toHaveTextContent("Low-level design · flow");
    expect(today).toHaveTextContent("today");
    expect(today).toHaveTextContent("6 min");
    expect(screen.getByRole("region", { name: "This week" })).toHaveTextContent("Last week doc");
    expect(screen.queryByText("<Blank>")).not.toBeInTheDocument();
  });

  it("narrows live, by family chip, and shows templates when asked", async () => {
    const { router } = open("/docs", ROWS);
    await screen.findByRole("region", { name: "Today" });

    await userEvent.type(screen.getByRole("searchbox", { name: "Search documents" }), "week");
    expect(screen.queryByText("Fresh doc")).not.toBeInTheDocument();
    expect(screen.getByText("Last week doc")).toBeInTheDocument();
    expect(router.state.location.search).toEqual({ q: "week" });
    await userEvent.clear(screen.getByRole("searchbox", { name: "Search documents" }));

    await userEvent.click(within(screen.getByRole("group", { name: "Type" })).getByRole("button", { name: /Low-level design/ }));
    expect(await screen.findByText("Fresh doc")).toBeInTheDocument();
    expect(screen.queryByText("Last week doc")).not.toBeInTheDocument();
    await userEvent.click(within(screen.getByRole("group", { name: "Type" })).getByRole("button", { name: /All/ }));

    await userEvent.click(screen.getByRole("checkbox", { name: "Show templates" }));
    expect(await screen.findByText("<Blank>")).toBeInTheDocument();
  });

  it("finds a doc by its text, newest first, and shows the matching line", async () => {
    const { calls } = open("/docs", ROWS, {
      "/api/search": {
        body: {
          query: "retry",
          hits: [
            { id: "docs/week", anchor: "limits", path: "stone/docs/week.md", lines: [5, 9], kind: "doc", snippet: "start of the section" },
            { id: "docs/fresh", anchor: null, path: "stone/docs/fresh.md", lines: [1, 4], kind: "doc", snippet: "start of the file" },
            { id: "docs/fresh", anchor: "more", path: "stone/docs/fresh.md", lines: [6, 9], kind: "doc", snippet: "again" },
          ],
          truncated: false,
        },
      },
      "/api/items/docs/week": { body: itemView("docs/week", { body: "# Last week doc\n\nFirst.\n\n## Limits\n\nThree words.\n\nThe retry count is 3.\n" }) },
      "/api/items/docs/fresh": { body: itemView("docs/fresh", { body: "# Fresh doc\n\nA retry waits.\n" }) },
    });
    await screen.findByRole("region", { name: "Today" });
    await userEvent.type(screen.getByRole("searchbox", { name: "Search documents" }), "retry");

    const week = await screen.findByRole("link", { name: "Last week doc" });
    await waitFor(() => expect(week).toHaveAttribute("href", "/docs/week#limits"));
    const older = screen.getByRole("region", { name: "This week" });
    expect(older).toHaveTextContent("The retry count is 3.");
    expect(older).not.toHaveTextContent("start of the section");
    expect(within(older).getByText("retry").tagName).toBe("MARK");
    const today = screen.getByRole("region", { name: "Today" });
    await waitFor(() => expect(today).toHaveTextContent("A retry waits."));
    expect(today.compareDocumentPosition(older) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(calls.filter((url) => url.startsWith("/api/search"))).toEqual(["/api/search?q=retry&type=doc&limit=1000"]);
  });

  it("still filters by title when the text search does not answer", async () => {
    open("/docs?q=week", ROWS, { "/api/search": { status: 500, body: { error: "down", kind: "internal" } } });
    expect(await screen.findByText("Last week doc")).toBeInTheDocument();
    expect(await screen.findByText(/only titles and summaries are searched/)).toBeInTheDocument();
  });

  it("sends nothing for text with no word in it", async () => {
    const { calls } = open("/docs?q=---", ROWS);
    expect(await screen.findByText(/No document matches/)).toBeInTheDocument();
    await new Promise((done) => setTimeout(done, 250));
    expect(calls.filter((url) => url.startsWith("/api/search"))).toEqual([]);
  });

  it("narrows by part of the product, and the address keeps the choice", async () => {
    const hld = {
      systems: [{ id: "arch/shop", title: "Shop", type: "system" }],
      containers: [
        { id: "arch/shop/api", title: "Orders API", type: "container", parent: "arch/shop" },
        { id: "arch/shop/idle", title: "Nothing here", type: "container", parent: "arch/shop" },
      ],
      connections: [],
      drawn: [],
      docs: [],
    };
    const { router } = open("/docs", ROWS, {
      "/api/arch/hld": { body: hld },
      "/api/items/arch/shop/api": {
        body: itemView("arch/shop/api", {
          type: "container",
          backlinks: [
            { id: "docs/fresh", anchor: null, path: "stone/docs/fresh.md", line: 3, kind: "links" },
            { id: "tasks/CS-1", anchor: null, path: "stone/tasks/CS-1.md", line: 3, kind: "links" },
          ],
        }),
      },
      "/api/items/arch/shop/idle": { body: itemView("arch/shop/idle", { type: "container" }) },
    });
    const parts = await screen.findByRole("group", { name: "Part" });
    expect(within(parts).queryByRole("button", { name: /Nothing here/ })).not.toBeInTheDocument();
    await userEvent.click(await within(parts).findByRole("button", { name: /Orders API/ }));
    await waitFor(() => expect(screen.queryByText("Last week doc")).not.toBeInTheDocument());
    expect(screen.getByText("Fresh doc")).toBeInTheDocument();
    expect(router.state.location.search).toEqual({ part: "arch/shop/api" });

    await userEvent.click(within(screen.getByRole("group", { name: "Type" })).getByRole("button", { name: /Low-level design/ }));
    await waitFor(() => expect(router.state.location.search).toEqual({ group: "lld", part: "arch/shop/api" }));
  });

  it("opens with a part from the address", async () => {
    open("/docs?part=arch/shop/api", ROWS, {
      "/api/arch/hld": { body: { systems: [], containers: [{ id: "arch/shop/api", title: "Orders API", type: "container" }], connections: [], drawn: [], docs: [] } },
      "/api/items/arch/shop/api": {
        body: itemView("arch/shop/api", { type: "container", backlinks: [{ id: "docs/week", anchor: null, path: "p", line: 1, kind: "links" }] }),
      },
    });
    expect(await screen.findByText("Last week doc")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Fresh doc")).not.toBeInTheDocument());
  });

  it("offers what was opened last", async () => {
    rememberDoc("docs/week");
    open("/docs", ROWS);
    const back = await screen.findByRole("region", { name: "Continue reading" });
    expect(within(back).getByRole("link", { name: "Last week doc" })).toHaveAttribute("href", "/docs/week");
  });

  it("lists the rows of a shelf with their status, and says so when a shelf is empty", async () => {
    open("/docs?shelf=decisions", ROWS, {
      "/api/items?type=decision": { body: [listRow("decisions/ADR-1", { title: "Use Rust", short: "ADR-1", type: "decision", status: "accepted" })] },
      "/api/items?type=plan": { body: [] },
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Decisions" })).toBeInTheDocument();
    expect(screen.getByText("Why the system is the way it is")).toBeInTheDocument();
    const row = await screen.findByRole("link", { name: "Use Rust" });
    expect(row).toHaveAttribute("href", "/items/decisions/ADR-1");
    expect(row.closest("li")).toHaveTextContent("ADR-1");
    expect(row.closest("li")).toHaveTextContent("accepted");
    expect(screen.queryByRole("searchbox", { name: "Search documents" })).toBeNull();
  });

  it("says so when a plan shelf holds nothing", async () => {
    open("/docs?shelf=plans", ROWS, { "/api/items?type=plan": { body: [] } });
    expect(await screen.findByRole("heading", { level: 1, name: "Plans" })).toBeInTheDocument();
    expect(await screen.findByText("No plans yet.")).toBeInTheDocument();
  });

  it("sorts by name on request", async () => {
    open("/docs?sort=name", ROWS);
    const links = await screen.findAllByRole("link", { name: /doc$/ });
    expect(links.map((link) => link.textContent).filter((text) => text?.endsWith(" doc"))).toEqual(["Fresh doc", "Last week doc"]);
    expect(screen.queryByRole("region", { name: "Today" })).not.toBeInTheDocument();
  });
});

describe("doc reader", () => {
  const BODY =
    "# PRD\n\nKind: flow · Arch item: [[arch/app]] · Repo: shop-web\nRelated: [[docs/other]]\n\n## Goals\n\nSee [[CS-1]] and [[docs/other#scope]].\n\n### Detail\n\nText.\n";
  const ROWS = [
    listRow("docs/lld-prd", { title: "PRD" }),
    listRow("docs/other", { title: "Other doc" }),
    listRow("tasks/CS-1", { title: "A task", type: "task", short: "CS-1", status: "doing" }),
    listRow("arch/app", { title: "The app", type: "container" }),
  ];
  const COMMIT = { sha: "abc1234def", date: "2026-11-02T10:00:00Z", author: "Ana", subject: "Rewrite the goals", refs: [] };

  function open(over: Parameters<typeof itemView>[1] = {}, history: unknown[] = [COMMIT]) {
    fakeApi({
      "/api/items": { body: ROWS },
      "/api/docs": {
        body: [
          doc("lld-prd", { title: "PRD", group: "lld", kind: "flow", updated: isoDaysAgo(3), minutes: 6 }),
          doc("other", { title: "Other doc", group: "lld", summary: "The other one" }),
        ],
      },
      "/api/history/docs/lld-prd": { body: { id: "docs/lld-prd", entries: history } },
      "/api/items/docs/lld-prd": {
        body: itemView("docs/lld-prd", {
          fields: { title: "PRD", status: "draft", links: ["arch/app"] },
          body: BODY,
          sections: [
            { anchor: "prd", title: "PRD", level: 1, start_line: 1, end_line: 12 },
            { anchor: "goals", title: "Goals", level: 2, start_line: 5, end_line: 12 },
            { anchor: "detail", title: "Detail", level: 3, start_line: 9, end_line: 12 },
          ],
          backlinks: [
            { id: "tasks/CS-1", anchor: "description", path: "stone/tasks/CS-1.md", line: 12, kind: "wikilink" },
            { id: "tasks/CS-1", anchor: "plan", path: "stone/tasks/CS-1.md", line: 20, kind: "wikilink" },
          ],
          stale: { reason: "src/app changed after the review", commits: 3 },
          ...over,
        }),
      },
    });
    render(<App router={createMemoryRouter(createQueryClient(), "/docs/lld-prd")} />);
  }

  it("shows the title once, then the byline, with no block of data above the text", async () => {
    open();
    expect(await screen.findAllByRole("heading", { level: 1, name: "PRD" })).toHaveLength(1);
    expect(await screen.findByText(/Updated 3 days ago · 6 min read · draft/)).toBeInTheDocument();
    expect(screen.queryByLabelText("About this document")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Linked from" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /Next in/ })).not.toBeInTheDocument();
    expect(screen.getByText("Repo: shop-web")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("src/app changed after the review");
  });

  it("links read as titles, and a numbered item keeps its short id", async () => {
    open();
    const article = await screen.findByRole("article");
    expect(await within(article).findByRole("link", { name: "CS-1" })).toHaveAttribute("href", "/tasks/CS-1");
    expect((await within(article).findAllByRole("link", { name: "Other doc" })).length).toBeGreaterThan(1);
    expect(within(article).queryByRole("link", { name: "docs/other#scope" })).not.toBeInTheDocument();
  });

  it("fills the frame's outline with the sections, then Connected to", async () => {
    open();
    const toc = await screen.findByRole("navigation", { name: "On this page" });
    expect(within(toc).getAllByRole("link").map((link) => link.textContent)).toEqual(["Goals", "Detail", "Connected to"]);
    expect(within(toc).getByRole("link", { name: "Goals" })).toHaveAttribute("href", "#goals");
    expect(within(toc).getByRole("link", { name: "Connected to" })).toHaveAttribute("href", "#connected-to");
    expect(await screen.findByRole("heading", { level: 2, name: "Connected to" })).toHaveAttribute("id", "connected-to");
  });

  it("ends with what it connects to, each entry saying how", async () => {
    open();
    const connected = await screen.findByRole("region", { name: "Connected to" });
    const documents = await within(connected).findByRole("region", { name: "Documents" });
    expect(within(documents).getByRole("link", { name: "Other doc" })).toHaveAttribute("href", "/docs/other");
    expect(documents).toHaveTextContent('Listed as related · Mentioned in "Goals"');

    const tasks = within(connected).getByRole("region", { name: "Tasks" });
    expect(within(tasks).getByRole("link", { name: "A task" })).toHaveAttribute("href", "/tasks/CS-1");
    expect(tasks).toHaveTextContent('Mentioned in "Goals" · Mentions this document in "Description" · and 1 more');
    expect(tasks).toHaveTextContent("doing");

    const architecture = within(connected).getByRole("region", { name: "Architecture" });
    expect(within(architecture).getByRole("link", { name: "The app" })).toHaveAttribute("href", "/architecture/arch/app");
    expect(architecture).toHaveTextContent("The part this document describes");

    const commits = await within(connected).findByRole("region", { name: "Commits" });
    expect(commits).toHaveTextContent("abc1234");
    expect(commits).toHaveTextContent("Rewrite the goals");
    expect(commits).toHaveTextContent("Changed this document on 2026-11-02, by Ana");
  });

  it("says so when nothing connects", async () => {
    open({ fields: { title: "PRD" }, body: "Plain text.\n", sections: [], backlinks: [], stale: undefined }, []);
    const connected = await screen.findByRole("region", { name: "Connected to" });
    expect(connected).toHaveTextContent("Nothing names this document yet");
    expect(within(connected).queryByRole("region", { name: "Commits" })).not.toBeInTheDocument();
  });

  it("places a low-level doc under Library and its family in the breadcrumb", async () => {
    open();
    const where = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(await within(where).findByRole("link", { name: "Low-level design" })).toHaveAttribute("href", "/docs?group=lld");
    expect(within(where).getByRole("link", { name: "Library" })).toBeInTheDocument();
    expect(within(where).queryByRole("link", { name: "The app" })).toBeNull();
    expect(where.querySelector('a[href^="/architecture"]')).toBeNull();
  });

  it("offers no write", async () => {
    open();
    await screen.findByRole("region", { name: "Connected to" });
    expect(screen.queryByRole("button", { name: /^(New|Edit|Save|Delete|Rename|Move)/ })).toBeNull();
  });
});
