import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { listRow } from "@/test/fixtures";
import { Markdown } from "./Markdown";
import { buildResolver } from "./links";

const resolver = buildResolver([
  listRow("decisions/ADR-3"),
  listRow("tasks/CS-1"),
  listRow("docs/prd", { type: "doc" }),
  listRow("docs/guide/intro", { type: "doc" }),
]);

function view(source: string, fromId = "docs/guide/intro") {
  return render(<Markdown source={source} resolver={resolver} fromId={fromId} />);
}

describe("markdown", () => {
  it("headings_carry_engine_anchors", () => {
    const { container } = view("## B\n\ntext\n\n## B\n\n## Why? (really)\n");
    expect(container.querySelector("h2#b")).not.toBeNull();
    expect(container.querySelector("h2#b-1")).not.toBeNull();
    expect(container.querySelector("h2#why-really")).not.toBeNull();
  });

  it("wikilink_targets_router", () => {
    view("See [[ADR-3#decision]], [[cs-001]] and [[docs/prd#data-model]].");
    expect(screen.getByRole("link", { name: "ADR-3#decision" })).toHaveAttribute("href", "/items/decisions/ADR-3#decision");
    expect(screen.getByRole("link", { name: "cs-001" })).toHaveAttribute("href", "/tasks/CS-1");
    expect(screen.getByRole("link", { name: "docs/prd#data-model" })).toHaveAttribute("href", "/docs/prd#data-model");
  });

  it("md_links_resolve_from_the_item", () => {
    view("[tasks](../../tasks/CS-1.md) and [sibling](../prd.md#goals) and [gone](missing.md)");
    expect(screen.getByRole("link", { name: "tasks" })).toHaveAttribute("href", "/tasks/CS-1");
    expect(screen.getByRole("link", { name: "sibling" })).toHaveAttribute("href", "/docs/prd#goals");
    expect(screen.queryByRole("link", { name: "gone" })).toBeNull();
    expect(screen.getByText("gone")).toHaveAttribute("data-broken", "true");
  });

  it("broken_links_are_marked", () => {
    view("[[CS-99]] and [[tasks/CS-1]]");
    expect(screen.getByText("CS-99")).toHaveAttribute("data-broken", "true");
    expect(screen.getByRole("link", { name: "tasks/CS-1" })).toBeInTheDocument();
  });

  it("links_stay_unmarked_until_the_list_loads", () => {
    render(<Markdown source="[[CS-99]]" resolver={buildResolver([], false)} />);
    expect(screen.getByText("CS-99")).not.toHaveAttribute("data-broken");
  });

  it("code_links_become_chips", () => {
    view("Calls [[code:Shop.Orders.Place]] here.");
    const chip = screen.getByRole("link", { name: "Shop.Orders.Place" });
    expect(chip).toHaveAttribute("href", "/code?target=Shop.Orders.Place");
    expect(chip).toHaveAttribute("data-code-chip", "true");
  });

  it("links_in_code_are_left_alone", () => {
    view("Write `[[CS-1]]` like this:\n\n```\n[[CS-1]]\n```\n");
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("external_images_do_not_load", () => {
    const { container } = view("![logo](https://example.com/x.png) ![local](/x.png)");
    expect(container.querySelectorAll("img")).toHaveLength(1);
    expect(container.querySelector("img")).toHaveAttribute("src", "/x.png");
  });
});
