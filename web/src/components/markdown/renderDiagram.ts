export type Diagram = { svg: string } | { error: string };

// Mermaid keeps global state while it renders, so diagrams go through one at a time.
let queue: Promise<unknown> = Promise.resolve();
let counter = 0;

export function nextDiagramId(): string {
  counter += 1;
  return `mermaid-${counter}`;
}

async function draw(chart: string, dark: boolean, id: string): Promise<Diagram> {
  try {
    // Loaded on first use: Mermaid is by far the largest dependency and most pages have no diagram.
    const { default: mermaid } = await import("mermaid");
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      suppressErrorRendering: true,
      theme: dark ? "dark" : "default",
    });
    const { svg } = await mermaid.render(id, chart);
    return { svg };
  } catch (err) {
    // A failed render can leave its scratch element in the page.
    document.getElementById(`d${id}`)?.remove();
    return { error: err instanceof Error ? err.message : String(err) };
  }
}

export function renderDiagram(chart: string, dark: boolean, id: string): Promise<Diagram> {
  const next = queue.then(
    () => draw(chart, dark, id),
    () => draw(chart, dark, id),
  );
  queue = next;
  return next;
}
