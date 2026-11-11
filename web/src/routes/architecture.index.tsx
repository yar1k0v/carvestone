import { createFileRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { C4Page, ComponentTrail } from "@/components/architecture/ArchitecturePage";
import { searchText } from "@/components/architecture/symbol/href";
import { AppLink } from "@/components/markdown/AppLink";
import { cn } from "@/lib/utils";
import { useWidePage } from "@/shell/pageFrame";

interface ArchitectureSearch {
  view?: "solution";
  system?: string;
  container?: string;
}

export const Route = createFileRoute("/architecture/")({
  validateSearch: (search: Record<string, unknown>): ArchitectureSearch => {
    if (search.view !== "solution") return {};
    const system = searchText(search.system);
    const container = searchText(search.container);
    return {
      view: "solution",
      ...(system === undefined ? {} : { system }),
      ...(container === undefined ? {} : { container }),
    };
  },
  component: ArchitectureRoute,
});

/** The two views of the system; a component diagram adds where it sits at the right. */
function SystemViews({ view, trail }: { view: "system" | "solution"; trail?: ReactNode }) {
  const tab = "border-b-2 pb-[7px]";
  const links: { view: "system" | "solution"; label: string; href: string }[] = [
    { view: "system", label: "System architecture", href: "/architecture" },
    { view: "solution", label: "Solution architecture", href: "/architecture?view=solution" },
  ];
  return (
    <nav aria-label="System views" className="flex flex-wrap items-end gap-x-5 border-b font-medium text-muted-foreground">
      {links.map((entry) => (
        <AppLink
          key={entry.view}
          href={entry.href}
          aria-current={view === entry.view ? "page" : undefined}
          className={cn(tab, view === entry.view ? "border-primary text-primary" : "border-transparent hover:text-foreground")}
        >
          {entry.label}
        </AppLink>
      ))}
      {trail}
    </nav>
  );
}

function ArchitectureRoute() {
  useWidePage();
  const { view, system, container } = Route.useSearch();
  return (
    <div className="space-y-4 text-sm leading-[1.4]">
      <SystemViews view={view === "solution" ? "solution" : "system"} trail={container === undefined ? null : <ComponentTrail container={container} />} />
      <C4Page view={view} system={system} container={container} />
    </div>
  );
}
