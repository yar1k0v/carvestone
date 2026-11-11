import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connectEvents, type EventsState, type EventSourceLike } from "./events";
import { keys } from "./keys";

class FakeSource implements EventSourceLike {
  static all: FakeSource[] = [];
  closed = false;
  private listeners = new Map<string, Array<(event: Event) => void>>();

  constructor(readonly url: string) {
    FakeSource.all.push(this);
  }
  addEventListener(type: string, listener: (event: Event) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  close() {
    this.closed = true;
  }
  emit(type: string, event: Event) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
  message(type: string, data: unknown) {
    this.emit(type, new MessageEvent(type, { data: JSON.stringify(data) }));
  }
}

const last = () => FakeSource.all[FakeSource.all.length - 1]!;

function setup() {
  const client = new QueryClient();
  const states: EventsState[] = [];
  const stop = connectEvents(client, (state) => states.push(state), {
    createSource: (url) => new FakeSource(url),
    baseDelay: 1000,
    maxDelay: 4000,
  });
  return { client, states, stop, state: () => states[states.length - 1]! };
}

const invalidated = (client: QueryClient, key: readonly unknown[]) =>
  client.getQueryState(key)?.isInvalidated === true;

beforeEach(() => {
  FakeSource.all = [];
  vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

describe("items events", () => {
  it("items_event_invalidates_queries", () => {
    const { client, stop } = setup();
    client.setQueryData(keys.items.get("tasks/CS-4"), {});
    client.setQueryData(keys.items.get("CS-4"), {});
    client.setQueryData(keys.items.get("tasks/CS-5"), {});
    client.setQueryData(keys.items.list({ type: "task" }), []);
    client.setQueryData(keys.code("find", { target: "X" }), {});

    last().message("items", { ids: ["tasks/CS-4", "CS-4"] });

    expect(invalidated(client, keys.items.get("tasks/CS-4"))).toBe(true);
    expect(invalidated(client, keys.items.get("CS-4"))).toBe(true);
    expect(invalidated(client, keys.items.list({ type: "task" }))).toBe(true);
    expect(invalidated(client, keys.items.get("tasks/CS-5"))).toBe(false);
    expect(invalidated(client, keys.code("find", { target: "X" }))).toBe(false);
    stop();
  });

  it("a code event invalidates code and arch queries only", () => {
    const { client, stop } = setup();
    client.setQueryData(keys.code("find", { target: "X" }), {});
    client.setQueryData(keys.arch(), {});
    client.setQueryData(keys.items.list(), []);

    last().message("code", { paths: ["crates/core/src/lib.rs"] });

    expect(invalidated(client, keys.code("find", { target: "X" }))).toBe(true);
    expect(invalidated(client, keys.arch())).toBe(true);
    expect(invalidated(client, keys.items.list())).toBe(false);
    stop();
  });
});

describe("connection", () => {
  it("stream_drop_shows_offline_and_retries", () => {
    const { states, state, stop } = setup();
    last().emit("open", new Event("open"));
    expect(state().status).toBe("online");

    // First drop: offline, retry after 1 s.
    last().emit("error", new Event("error"));
    expect(state().status).toBe("offline");
    expect(FakeSource.all).toHaveLength(1);
    vi.advanceTimersByTime(999);
    expect(FakeSource.all).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeSource.all).toHaveLength(2);

    // The retry fails too: the delay doubles.
    last().emit("error", new Event("error"));
    vi.advanceTimersByTime(1999);
    expect(FakeSource.all).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeSource.all).toHaveLength(3);

    // Back online: the delay resets and the missed events are covered by a refetch.
    last().emit("open", new Event("open"));
    expect(state().status).toBe("online");
    last().emit("error", new Event("error"));
    vi.advanceTimersByTime(1000);
    expect(FakeSource.all).toHaveLength(4);
    expect(states.some((s) => s.status === "offline")).toBe(true);
    stop();
  });

  it("a reconnect invalidates everything once", () => {
    const { client, stop } = setup();
    last().emit("open", new Event("open"));
    client.setQueryData(keys.config(), {});
    last().emit("error", new Event("error"));
    vi.advanceTimersByTime(1000);
    expect(invalidated(client, keys.config())).toBe(false);
    last().emit("open", new Event("open"));
    expect(invalidated(client, keys.config())).toBe(true);
    stop();
  });

  it("a server refresh error keeps the stream and shows the sentence", () => {
    const { state, stop } = setup();
    last().emit("open", new Event("open"));
    last().message("error", { error: "index is locked; retry" });
    expect(state()).toEqual({ status: "online", refreshError: "index is locked; retry" });
    expect(FakeSource.all).toHaveLength(1);
    last().message("items", { ids: ["CS-1"] });
    expect(state().refreshError).toBeNull();
    stop();
  });

  it("a lagged event refetches every query", () => {
    const { client, stop } = setup();
    client.setQueryData(keys.config(), {});
    client.setQueryData(keys.items.get("tasks/CS-5"), {});
    last().message("lagged", {});
    expect(invalidated(client, keys.config())).toBe(true);
    expect(invalidated(client, keys.items.get("tasks/CS-5"))).toBe(true);
    expect(FakeSource.all).toHaveLength(1);
    stop();
  });

  it("stopping closes the stream and cancels the retry", () => {
    const { stop } = setup();
    const first = last();
    last().emit("error", new Event("error"));
    stop();
    vi.advanceTimersByTime(60_000);
    expect(FakeSource.all).toHaveLength(1);
    expect(first.closed).toBe(true);
  });
});
