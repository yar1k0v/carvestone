/** The page of a class, a method or a file, for a qualified name, a path or `path:line`. */
export function classHref(target: string): string {
  return `/architecture/class?target=${encodeURIComponent(target)}`;
}

/** The page of a module, for a `mod:<project>/<folder>/<k>` id. */
export function moduleHref(id: string): string {
  return `/architecture/module?id=${encodeURIComponent(id)}`;
}

/** A search value as text: the router reads `?target=123` as a number. */
export function searchText(value: unknown): string | undefined {
  if (typeof value === "number") return String(value);
  return typeof value === "string" && value.trim() !== "" ? value : undefined;
}
