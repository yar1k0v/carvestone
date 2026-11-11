import { createContext, useContext, useLayoutEffect } from "react";

export interface OutlineEntry {
  /** The element id of the heading, without `#`. */
  id: string;
  title: string;
  /** 2 for a section, 3 for a section inside it. */
  level?: number;
}

export interface Crumb {
  label: string;
  /** Left out for the current page and for a group with no page of its own. */
  href?: string;
}

/** What the frame takes from the page on show. Outside the frame, as in a lone component test, it goes nowhere. */
export interface FrameSetters {
  outline: (entries: OutlineEntry[]) => void;
  crumbs: (crumbs: Crumb[] | null) => void;
  wide: (wide: boolean) => void;
}

export const FrameContext = createContext<FrameSetters>({ outline: () => {}, crumbs: () => {}, wide: () => {} });

/** Fills "On this page" while the calling page is on show. Give each heading the same `id`. */
export function useOutline(entries: OutlineEntry[]): void {
  const { outline } = useContext(FrameContext);
  const key = JSON.stringify(entries);
  useLayoutEffect(() => {
    outline(JSON.parse(key) as OutlineEntry[]);
    return () => outline([]);
  }, [key, outline]);
}

/** Replaces the breadcrumbs worked out from the address, for a page that knows its place better. */
export function useCrumbs(crumbs: Crumb[]): void {
  const { crumbs: set } = useContext(FrameContext);
  const key = JSON.stringify(crumbs);
  useLayoutEffect(() => {
    set(JSON.parse(key) as Crumb[]);
    return () => set(null);
  }, [key, set]);
}

/** Lets the page use the full width, for a board or a diagram. */
export function useWidePage(): void {
  const { wide } = useContext(FrameContext);
  useLayoutEffect(() => {
    wide(true);
    return () => wide(false);
  }, [wide]);
}
