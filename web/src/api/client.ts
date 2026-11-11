import type { ErrorBody } from "./types/ErrorBody";

/** A failed request: the server's sentence, its error kind and the HTTP status. */
export class ApiError extends Error {
  readonly error: string;
  readonly kind: string;
  readonly status: number;

  constructor(error: string, kind: string, status: number) {
    super(error);
    this.name = "ApiError";
    this.error = error;
    this.kind = kind;
    this.status = status;
  }
}

export type QueryValue = string | number | boolean | null | undefined;
export type Query = Record<string, QueryValue>;

/** `path` plus the query pairs that carry a value; `undefined`, `null` and `""` are left out. */
export function withQuery(path: string, query?: Query): string {
  if (!query) return path;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null || value === "") continue;
    params.set(key, String(value));
  }
  const text = params.toString();
  return text ? `${path}?${text}` : path;
}

function isErrorBody(value: unknown): value is ErrorBody {
  return (
    typeof value === "object" &&
    value !== null &&
    "error" in value &&
    typeof value.error === "string" &&
    "kind" in value &&
    typeof value.kind === "string"
  );
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, init);
  } catch {
    throw new ApiError("the server did not answer; check that `stone serve` is running", "offline", 0);
  }
  const text = await response.text();
  let parsed: unknown = null;
  if (text !== "") {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = null;
    }
  }
  if (!response.ok) {
    if (isErrorBody(parsed)) throw new ApiError(parsed.error, parsed.kind, response.status);
    throw new ApiError(`the server answered ${response.status}`, "internal", response.status);
  }
  return parsed as T;
}

/** Typed access to the JSON API. The caller names the response type from `src/api/types`. */
export const api = {
  get: <T>(path: string, query?: Query): Promise<T> => request<T>(withQuery(path, query)),
  patch: <T>(path: string, body: unknown): Promise<T> =>
    request<T>(path, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
};
