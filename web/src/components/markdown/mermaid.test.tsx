import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listRow } from "@/test/fixtures";
import { Markdown } from "./Markdown";
import { buildResolver } from "./links";

// Mermaid measures text with the browser's layout, which jsdom lacks, so the library is replaced by
// a stand-in here; the syntax-error text is checked against the real library in mermaid.real.test.tsx.
const mermaid = vi.hoisted(() => ({ initialize: vi.fn(), render: vi.fn() }));
vi.mock("mermaid", () => ({ default: mermaid }));

const resolver = buildResolver([listRow("docs/prd")]);
const FENCE = "```mermaid\ngraph TD\n  A --> B\n```\n";

describe("mermaid fences", () => {
  beforeEach(() => {
    mermaid.initialize.mockReset();
    mermaid.render.mockReset();
  });

  it("fence_renders_a_diagram", async () => {
    mermaid.render.mockResolvedValue({ svg: '<svg data-testid="diagram"></svg>' });
    render(<Markdown source={FENCE} resolver={resolver} />);
    expect(await screen.findByTestId("diagram")).toBeInTheDocument();
    expect(mermaid.render.mock.calls[0]?.[1]).toBe("graph TD\n  A --> B");
    expect(mermaid.initialize).toHaveBeenCalledWith(expect.objectContaining({ securityLevel: "strict", theme: "default" }));
  });

  it("render_error_is_shown", async () => {
    mermaid.render.mockRejectedValue(new Error("Parse error on line 2"));
    render(<Markdown source={FENCE} resolver={resolver} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Parse error on line 2");
  });

  it("other_fences_stay_code", () => {
    const { container } = render(<Markdown source={"```ts\nconst a = 1;\n```\n"} resolver={resolver} />);
    expect(container.querySelector("pre code")).toHaveTextContent("const a = 1;");
    expect(mermaid.render).not.toHaveBeenCalled();
  });
});
