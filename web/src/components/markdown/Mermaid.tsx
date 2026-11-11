import { useEffect, useState } from "react";
import { type Diagram, nextDiagramId, renderDiagram } from "./renderDiagram";
import { useIsDark } from "./useIsDark";

/** A Mermaid diagram. The library loads on first use and follows the light or dark theme. */
export function Mermaid({ chart }: { chart: string }) {
  const dark = useIsDark();
  const [id] = useState(nextDiagramId);
  const [diagram, setDiagram] = useState<Diagram | null>(null);

  useEffect(() => {
    let current = true;
    void renderDiagram(chart, dark, id).then((result) => {
      if (current) setDiagram(result);
    });
    return () => {
      current = false;
    };
  }, [chart, dark, id]);

  if (diagram === null) {
    return <p className="my-4 text-sm text-muted-foreground">Rendering diagram...</p>;
  }
  if ("error" in diagram) {
    return (
      <div role="alert" className="my-4 rounded-md border border-destructive/50 p-3 text-sm">
        <p className="font-medium text-destructive">The diagram does not render</p>
        <pre className="mt-2 overflow-x-auto whitespace-pre-wrap font-mono text-xs">{diagram.error}</pre>
        <pre className="mt-2 overflow-x-auto rounded bg-muted p-2 font-mono text-xs">{chart}</pre>
      </div>
    );
  }
  return (
    <div
      className="my-4 flex justify-center overflow-x-auto"
      role="img"
      aria-label="Diagram"
      // Mermaid's own output, built with securityLevel "strict" (sanitised by the library).
      dangerouslySetInnerHTML={{ __html: diagram.svg }}
    />
  );
}
