import type { Query } from "./client";

/**
 * Query keys. The first element is the family the event hook invalidates by, so every view builds
 * its keys here: `items` (`list` and `get`), `search`, `code`, `arch`, `history`, `timeline`,
 * `config`.
 */
export const keys = {
  items: {
    list: (params?: Query) => ["items", "list", params ?? {}] as const,
    /** `id` is the id the route uses, a path id or a short id. */
    get: (id: string, section?: string) => ["items", "get", id, section ?? null] as const,
  },
  /** In the `items` family, so a changed file refetches it. */
  docs: () => ["items", "docs"] as const,
  /** In the `items` family, so a changed file refetches it. */
  product: () => ["items", "product"] as const,
  feature: (id: string) => ["items", "feature", id] as const,
  taskFeatures: (id: string) => ["items", "task-features", id] as const,
  search: (params: Query) => ["search", params] as const,
  code: (op: string, params: Query) => ["code", op, params] as const,
  arch: () => ["arch", "graph"] as const,
  hld: () => ["arch", "hld"] as const,
  lld: (container: string) => ["arch", "lld", container] as const,
  c4Context: () => ["arch", "c4", "context"] as const,
  c4Container: (system?: string) => ["arch", "c4", "container", system ?? null] as const,
  c4Component: (container: string) => ["arch", "c4", "component", container] as const,
  history: (id: string, params?: Query) => ["history", id, params ?? {}] as const,
  taskCommits: (id: string) => ["history", "task", id] as const,
  timeline: (params?: Query) => ["timeline", params ?? {}] as const,
  activity: (params: Query) => ["timeline", "activity", params] as const,
  config: () => ["config"] as const,
};
