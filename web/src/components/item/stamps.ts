import type { JsonValue } from "@/api/types/serde_json/JsonValue";

/** Who did something and when: the shape of `generated` and of each `verified` entry. */
export interface Stamp {
  by: string;
  at: string;
}

/** The day of a date or a timestamp; any other text is returned as it is. */
export function dayOf(at: string): string {
  return /^\d{4}-\d{2}-\d{2}T/.test(at) ? at.slice(0, 10) : at;
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
