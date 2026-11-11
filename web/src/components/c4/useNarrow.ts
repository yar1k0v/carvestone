import { useSyncExternalStore } from "react";

const PHONE = "(max-width: 639px)";

function subscribe(onChange: () => void): () => void {
  if (typeof window.matchMedia !== "function") return () => {};
  const query = window.matchMedia(PHONE);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

/** Whether the screen is phone-wide. A browser without `matchMedia`, like jsdom, counts as wide. */
export function useNarrow(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => typeof window.matchMedia === "function" && window.matchMedia(PHONE).matches,
    () => false,
  );
}
