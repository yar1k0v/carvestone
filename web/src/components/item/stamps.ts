import type { JsonValue } from "@/api/types/serde_json/JsonValue";

/** Who did something and when: the shape of `generated` and of each `verified` entry. */
export interface Stamp {
  by: string;
  at: string;
}

export function asStamp(value: JsonValue | undefined): Stamp | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const { by, at } = value;
  return typeof by === "string" && typeof at === "string" ? { by, at } : null;
}

export function asStamps(value: JsonValue | undefined): Stamp[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry) => {
    const stamp = asStamp(entry);
    return stamp ? [stamp] : [];
  });
}
