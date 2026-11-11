import { useQuery } from "@tanstack/react-query";
import { type Query, api } from "@/api/client";
import { keys } from "@/api/keys";

/** One `GET /api/code/<op>` call. `enabled: false` waits until the form has what the op needs. */
export function useCode<T>(op: string, params: Query, enabled = true) {
  return useQuery({
    queryKey: keys.code(op, params),
    queryFn: () => api.get<T>(`/api/code/${op}`, params),
    enabled,
  });
}

/** The engine's answer for a name that fits several symbols, split into the name and its choices. */
export interface Ambiguity {
  name: string;
  options: string[];
}

const AMBIGUOUS = /^(.*) matches \d+ symbols; use one of: (.+)$/s;

/** Reads the candidate list out of the engine's `X matches N symbols; use one of: A, B, ...` sentence. */
export function ambiguityOf(error: Error): Ambiguity | null {
  const match = AMBIGUOUS.exec(error.message);
  if (match?.[1] === undefined || match[2] === undefined) return null;
  const options = match[2]
    .split(", ")
    .map((option) => option.trim())
    .filter((option) => option !== "" && option !== "...");
  return options.length === 0 ? null : { name: match[1], options };
}
