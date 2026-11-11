import type { MouseEvent } from "react";

/**
 * A click handler that sends a plain left click on an in-app link to the router and leaves
 * new-tab clicks to the browser. Without `go` the link works as a normal one.
 */
export function internalClick(go: ((href: string) => void) | undefined, href: string) {
  return (event: MouseEvent<HTMLAnchorElement>) => {
    if (!go || event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    go(href);
  };
}
