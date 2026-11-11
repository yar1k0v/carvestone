import { serviceHref } from "@/components/graph/serviceHref";
import type { Resolver } from "@/components/markdown/links";
import type { C4Diagram, C4Element } from "./parse";

/** The address of the container diagram of one system. */
export function systemHref(id: string): string {
  return `/architecture?view=solution&system=${encodeURIComponent(id)}`;
}

/** The address of the component diagram of one container. */
export function containerHref(id: string): string {
  return `/architecture?view=solution&container=${encodeURIComponent(id)}`;
}

/** Where a box opens: a system its containers, a container its components, a component its inside view. People and outside systems open nothing. */
export function hrefOfElement(element: C4Element, id: string): string | undefined {
  if (element.external) return undefined;
  switch (element.kind) {
    case "system":
      return systemHref(id);
    case "container":
      return containerHref(id);
    case "component":
      return serviceHref(id);
    default:
      return undefined;
  }
}

/** The address a linked item opens, by the id the resolver finds for each box's `$link`. */
export function viewHrefs(diagram: C4Diagram, resolver: Resolver): (id: string) => string | undefined {
  const byId = new Map<string, string>();
  for (const element of diagram.elements) {
    if (element.link === undefined) continue;
    const id = resolver.wiki(element.link)?.id;
    const href = id === undefined ? undefined : hrefOfElement(element, id);
    if (id !== undefined && href !== undefined) byId.set(id, href);
  }
  return (id) => byId.get(id);
}
