import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "@/App";
import type { Trust } from "@/api/types/Trust";
import { createMemoryRouter, createQueryClient } from "@/router";
import { fakeApi } from "@/test/fakeApi";
import { itemView, listRow } from "@/test/fixtures";
import { TrustBadge } from "./TrustBadge";

afterEach(() => vi.unstubAllGlobals());

function open(path: string) {
  const router = createMemoryRouter(createQueryClient(), path);
  render(<App router={router} />);
  return router;
}

const TASK = itemView("tasks/CS-2", {
  type: "task",
  trust: "human-reviewed",
  fields: {
    title: "Second task",
    status: "in-progress",
    milestone: "M-1",
    verified: [
      { by: "human:yaroslav", at: "2026-10-16T12:00:00+00:00" },
      { by: "claude-code", at: "2026-10-20" },
    ],
    generated: { by: "claude-code", at: "2026-10-01T09:00:00+00:00" },
  },
  body: "## Description\n\nDo it.\n",
  sections: [{ anchor: "description", title: "Description", level: 2, start_line: 8, end_line: 10 }],
  changed_files: [{ path: "src/lib.rs", status: "M", source: "commit", sha: "abcdef1234" }],
});

describe("item page", () => {
  it("short_id_redirects", async () => {
    fakeApi({
      "/api/items": { body: [listRow("tasks/CS-2"), listRow("milestones/M-1")] },
      "/api/items/CS-2": { body: TASK },
    });
    const router = open("/items/CS-2");
    await waitFor(() => expect(router.state.location.pathname).toBe("/items/tasks/CS-2"));
    expect(await screen.findByRole("heading", { name: "Second task" })).toBeInTheDocument();
  });

  it("not_found_shows_suggestion", async () => {
    fakeApi({
      "/api/items": { body: [] },
      "/api/items/ADR-11": { status: 404, body: { error: "ADR-11 not found. Did you mean ADR-1?", kind: "not_found" } },
    });
    open("/items/ADR-11");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("ADR-11 not found. Did you mean ADR-1?");
    expect(within(alert).getByRole("link", { name: "ADR-1" })).toHaveAttribute("href", "/items/ADR-1");
  });

  it("provenance_lists_all_verified", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/items/tasks/CS-2": { body: TASK } });
    open("/items/tasks/CS-2");
    expect(await screen.findByRole("heading", { name: "Second task" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Provenance" }));

    const verified = screen.getByRole("region", { name: "Verified" });
    expect(within(verified).getAllByRole("listitem")).toHaveLength(2);
    expect(verified).toHaveTextContent("human:yaroslav");
    expect(verified).toHaveTextContent("claude-code");
    expect(verified).not.toHaveTextContent("12:00");
    const generated = screen.getByRole("region", { name: "Generated" });
    expect(generated).toHaveTextContent("2026-10-01");
    expect(generated).not.toHaveTextContent("09:00");
    // The badge is the engine's value, not recomputed from the stamps.
    expect(screen.getAllByText("human-reviewed").length).toBeGreaterThan(0);
  });

  it("history_entries_render", async () => {
    const { calls } = fakeApi({
      "/api/items": { body: [] },
      "/api/items/tasks/CS-2": { body: TASK },
      "/api/history/tasks/CS-2": {
        body: {
          id: "tasks/CS-2",
          entries: [
            {
              sha: "0123456789abcdef",
              date: "2026-10-20",
              author: "Yaroslav",
              subject: "feat: do it",
              refs: ["CS-2"],
              diff: "@@ -1 +1 @@\n-old line\n+new line\n",
            },
          ],
        },
      },
    });
    const { container } = render(<App router={createMemoryRouter(createQueryClient(), "/items/tasks/CS-2")} />);
    await userEvent.click(await screen.findByRole("tab", { name: "History" }));

    expect(await screen.findByText("0123456")).toBeInTheDocument();
    expect(screen.getByText("2026-10-20")).toBeInTheDocument();
    expect(screen.getByText("Yaroslav")).toBeInTheDocument();
    expect(screen.getByText("feat: do it")).toBeInTheDocument();
    expect(container.querySelector("details")).not.toBeNull();
    expect(screen.getByText("+new line")).toBeInTheDocument();
    expect(calls).toContain("/api/history/tasks/CS-2?diff=true");

    // Choosing a section narrows the history to that anchor.
    await userEvent.selectOptions(screen.getByRole("combobox"), "description");
    await waitFor(() => expect(calls).toContain("/api/history/tasks/CS-2?section=description&diff=true"));
  });

  it("task_lists_changed_files", async () => {
    fakeApi({ "/api/items": { body: [] }, "/api/items/tasks/CS-2": { body: TASK } });
    open("/items/tasks/CS-2");
    await userEvent.click(await screen.findByRole("tab", { name: "Changed files (1)" }));
    expect(screen.getByText("src/lib.rs")).toBeInTheDocument();
    expect(screen.getByText(/in a commit abcdef1/)).toBeInTheDocument();
  });

  it("component_shows_libraries_dependencies_and_drift", async () => {
    const component = itemView("arch/orders", {
      type: "component",
      fields: { title: "Orders" },
      libraries: [{ library: "mediatr", version: "12" }, { library: "serilog", version: null }],
      code_dependencies: [{ from: "arch/orders", to: "arch/billing", edges: 3, first: ["src/orders.rs", 14], inferred_only: false }],
    });
    fakeApi({
      "/api/items": { body: [] },
      "/api/items/arch/orders": { body: component },
      "/api/arch/graph": {
        body: {
          nodes: [],
          uses: [],
          code_edges: [],
          diagrams: [],
          drift: [
            { kind: "code", from: "arch/orders", to: "arch/billing", path: "stone/arch/orders.md", line: 6, rule: "arch-drift", message: "code uses arch/billing but uses does not list it" },
            { kind: "uses", from: "arch/other", to: "arch/x", path: "stone/arch/other.md", line: 1, rule: "arch-drift", message: "not mine" },
          ],
        },
      },
    });
    open("/items/arch/orders");
    expect(await screen.findByText(/Libraries \(look up in Docs MCP with search_docs\)/)).toHaveTextContent("mediatr@12, serilog");
    const deps = await screen.findByRole("region", { name: "Code dependencies" });
    expect(within(deps).getByRole("link", { name: "arch/billing" })).toHaveAttribute("href", "/items/arch/billing");
    expect(deps).toHaveTextContent("3 edges");
    const drift = await screen.findByRole("region", { name: "Drift" });
    await waitFor(() => expect(drift).toHaveTextContent("code uses arch/billing"));
    expect(drift).not.toHaveTextContent("not mine");
  });
});

describe("trust badge", () => {
  it("trust_badge_from_engine_value", () => {
    const { rerender } = render(<TrustBadge trust="unverified" />);
    for (const trust of ["unverified", "machine-confirmed", "human-reviewed"] satisfies Trust[]) {
      rerender(<TrustBadge trust={trust} />);
      expect(screen.getByText(trust)).toHaveAttribute("data-trust", trust);
    }
  });
});
