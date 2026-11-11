import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { listRow } from "@/test/fixtures";
import { Markdown } from "./Markdown";
import { buildResolver } from "./links";

describe("mermaid syntax errors", () => {
  it("mermaid_error_shown", async () => {
    const source = "```mermaid\nthis is not a diagram\n```\n";
    render(<Markdown source={source} resolver={buildResolver([listRow("docs/prd")])} />);
    const alert = await screen.findByRole("alert", {}, { timeout: 15_000 });
    expect(alert).toHaveTextContent("The diagram does not render");
    // The library's own sentence, not a blank box.
    expect(alert.textContent?.length ?? 0).toBeGreaterThan("The diagram does not render".length + "this is not a diagram".length);
  }, 20_000);
});
