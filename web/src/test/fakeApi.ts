import { vi } from "vitest";

/** A canned answer: a status and a JSON body. */
export interface Reply {
  status?: number;
  body: unknown;
}

/**
 * Replaces `fetch` with a table of path to answer (query string included in the key when the test
 * wants to tell calls apart; otherwise the bare path matches). A key may start with a method
 * (`"GET /api/code/native"`) to answer only that method; the plain key answers any. An unknown
 * path answers 404.
 */
export function fakeApi(routes: Record<string, Reply>) {
  const calls: string[] = [];
  const fetchMock = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push(url);
    const path = url.split("?")[0] ?? url;
    const method = init?.method ?? "GET";
    const reply = routes[`${method} ${url}`] ?? routes[`${method} ${path}`] ?? routes[url] ?? routes[path] ?? {
      status: 404,
      body: { error: `${path} not found`, kind: "not_found" },
    };
    return Promise.resolve(
      new Response(JSON.stringify(reply.body), {
        status: reply.status ?? 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
  });
  vi.stubGlobal("fetch", fetchMock);
  return { calls, fetchMock };
}
