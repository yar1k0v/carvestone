export type C4LegendLevel = "context" | "container" | "component";

/** What each kind of box and line means at one level; the words are the owner-approved boards'. */
function linesOf(level: C4LegendLevel, container: string | undefined): string[] {
  switch (level) {
    case "context":
      return [
        "Rounded box: a person",
        "Blue box: the system. Click it to open the next level",
        "Dashed box: a system outside this one",
        "Arrow: who starts it. The label says what, and over what protocol",
      ];
    case "container":
      return [
        "Rounded box: a person",
        "Blue box: an app or service. Click to open the next level",
        "White box: a data store or message bus",
        "Dashed box: outside this system",
        "Lines are written in Stone. Code can only prove build-time links",
      ];
    case "component":
      return [
        `Blue boxes: components of ${container ?? "the container"}, drawn from the code. Click one to open its modules and classes`,
        "White boxes: shared libraries it depends on",
        "Numbers: references the code index proved. Lines from other containers are written in Stone",
      ];
  }
}

/** The legend under a diagram. */
export function C4Legend({ level, container }: { level: C4LegendLevel; container?: string | undefined }) {
  return (
    <section aria-label="Legend" className="space-y-0.5 text-[13px] leading-5 text-muted-foreground">
      <h2 className="font-semibold text-foreground">Legend</h2>
      <ul>
        {linesOf(level, container).map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
    </section>
  );
}
