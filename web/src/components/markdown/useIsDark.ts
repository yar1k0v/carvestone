import { useSyncExternalStore } from "react";

function subscribe(onChange: () => void): () => void {
  // The theme provider toggles the `dark` class on <html>; watching it covers light, dark and system.
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
  return () => observer.disconnect();
}

/** Whether the page is dark right now. */
export function useIsDark(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => document.documentElement.classList.contains("dark"),
    () => false,
  );
}
