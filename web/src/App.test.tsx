import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { createMemoryRouter, createQueryClient } from "./router";
import { fakeApi } from "./test/fakeApi";

afterEach(() => vi.unstubAllGlobals());

// jsdom has no EventSource, so the shell starts offline; the router and nav are what is under test.
describe("app", () => {
  it("app_renders", async () => {
    fakeApi({ "/api/items": { body: [] } });
    render(<App router={createMemoryRouter(createQueryClient(), "/")} />);
    expect(await screen.findByRole("heading", { level: 1, name: "Now" })).toBeInTheDocument();
    const spaces = screen.getAllByRole("navigation", { name: "Spaces" })[0] as HTMLElement;
    for (const label of ["Now", "Product", "System", "Library"]) {
      expect(spaces).toHaveTextContent(label);
    }
    const nav = screen.getByRole("navigation", { name: "Main" });
    for (const label of ["Today", "Board", "Milestones", "Plans", "Shipped"]) {
      expect(nav).toHaveTextContent(label);
    }
    // One light in the top bar and one in the phone bar.
    const lights = screen.getAllByRole("status");
    expect(lights).toHaveLength(2);
    for (const light of lights) expect(light).toHaveTextContent("offline");
  });
});
