import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import type { CodeEvent } from "./types/CodeEvent";
import type { ErrorEvent as RefreshErrorEvent } from "./types/ErrorEvent";
import type { ItemsEvent } from "./types/ItemsEvent";

export type ConnectionStatus = "connecting" | "online" | "offline";

export interface EventsState {
  status: ConnectionStatus;
  /** The server's sentence when its last refresh failed; cleared by the next event. */
  refreshError: string | null;
}

/** The part of `EventSource` the hook uses, so tests can pass a fake. */
export interface EventSourceLike {
  addEventListener(type: string, listener: (event: Event) => void): void;
  close(): void;
}

export interface EventsOptions {
  url?: string;
  createSource?: (url: string) => EventSourceLike;
  /** First reconnect delay in ms; it doubles per failed attempt up to `maxDelay`. */
  baseDelay?: number;
  maxDelay?: number;
}

const initial: EventsState = { status: "connecting", refreshError: null };

/** `tasks/CS-4` and `CS-4` name the same item; compare on the last segment. */
function shortId(id: string): string {
  return id.slice(id.lastIndexOf("/") + 1);
}

/**
 * An `items` event: refetch those items and everything that lists or searches items. The event
 * carries path ids and short ids; a cached query may be keyed by either.
 */
export function invalidateForItems(client: QueryClient, ids: string[]): void {
  const shorts = new Set(ids.map(shortId));
  void client.invalidateQueries({
    predicate: ({ queryKey }) => {
      const [family, kind, id] = queryKey;
      if (family === "items" && kind === "get") {
        return typeof id === "string" && shorts.has(shortId(id));
      }
      return family === "items" || family === "search" || family === "history" || family === "timeline" || family === "arch";
    },
  });
}

/** A `code` event: code answers and the architecture map's code edges are stale. */
export function invalidateForCode(client: QueryClient): void {
  void client.invalidateQueries({
    predicate: ({ queryKey }) => queryKey[0] === "code" || queryKey[0] === "arch" || queryKey[0] === "search",
  });
}

function parse<T>(event: Event): T | null {
  const data = (event as MessageEvent<unknown>).data;
  if (typeof data !== "string") return null;
  try {
    return JSON.parse(data) as T;
  } catch {
    return null;
  }
}

/**
 * Opens `/api/events` and keeps it open: query caches are invalidated per event, and a dropped
 * stream shows `offline` and reconnects with a growing delay. Returns a function that stops it.
 */
export function connectEvents(
  client: QueryClient,
  onState: (state: EventsState) => void,
  options: EventsOptions = {},
): () => void {
  const url = options.url ?? "/api/events";
  const create = options.createSource ?? ((u: string) => new EventSource(u));
  const baseDelay = options.baseDelay ?? 1000;
  const maxDelay = options.maxDelay ?? 30_000;

  let source: EventSourceLike | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let attempts = 0;
  let stopped = false;
  let wasOffline = false;
  let state = initial;
  const set = (next: Partial<EventsState>) => {
    state = { ...state, ...next };
    onState(state);
  };

  const open = () => {
    if (typeof EventSource === "undefined" && options.createSource === undefined) {
      set({ status: "offline" });
      return;
    }
    const current = create(url);
    source = current;
    current.addEventListener("open", () => {
      attempts = 0;
      // Events sent while the stream was down are lost; refetch everything once.
      if (wasOffline) void client.invalidateQueries();
      wasOffline = false;
      set({ status: "online", refreshError: null });
    });
    current.addEventListener("items", (event) => {
      const body = parse<ItemsEvent>(event);
      if (body === null || !Array.isArray(body.ids)) return;
      invalidateForItems(client, body.ids);
      set({ refreshError: null });
    });
    current.addEventListener("code", (event) => {
      const body = parse<CodeEvent>(event);
      if (body === null || !Array.isArray(body.paths)) return;
      invalidateForCode(client);
      set({ refreshError: null });
    });
    // The server dropped events this client was too slow to read; nothing says which, so refetch all.
    current.addEventListener("lagged", () => {
      void client.invalidateQueries();
      set({ refreshError: null });
    });
    // `error` is both the browser's connection failure (a plain Event) and the server's refresh
    // failure (a MessageEvent with a body); the stream stays up for the second.
    current.addEventListener("error", (event) => {
      const body = parse<RefreshErrorEvent>(event);
      if (body !== null && typeof body.error === "string") {
        set({ refreshError: body.error });
        return;
      }
      current.close();
      if (source === current) source = null;
      wasOffline = true;
      set({ status: "offline" });
      if (stopped) return;
      const delay = Math.min(baseDelay * 2 ** attempts, maxDelay);
      attempts += 1;
      timer = setTimeout(open, delay);
    });
  };

  open();
  return () => {
    stopped = true;
    clearTimeout(timer);
    source?.close();
    source = null;
  };
}

/** The shell calls this once; it returns the connection state for the top bar. */
export function useEvents(options?: EventsOptions): EventsState {
  const client = useQueryClient();
  const [state, setState] = useState<EventsState>(initial);
  const { url, createSource, baseDelay, maxDelay } = options ?? {};
  useEffect(
    () => connectEvents(client, setState, { url, createSource, baseDelay, maxDelay }),
    [client, url, createSource, baseDelay, maxDelay],
  );
  return state;
}
