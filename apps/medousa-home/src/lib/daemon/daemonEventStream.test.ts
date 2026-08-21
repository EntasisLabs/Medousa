import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  order: [] as string[],
  start: vi.fn(async (..._args: unknown[]) => "handle"),
  cancel: vi.fn(async (..._args: unknown[]) => undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.order.push(`listen:${name.split("/").pop()}`);
    mocks.listeners.set(name, handler);
    return () => mocks.listeners.delete(name);
  }),
}));

vi.mock("$lib/daemon/contractClient", () => ({
  daemonStreamStart: (...args: unknown[]) => {
    mocks.order.push("start");
    return mocks.start(...args);
  },
  daemonStreamCancel: (...args: unknown[]) => mocks.cancel(...args),
}));

vi.mock("$lib/window", () => ({ isTauri: () => true }));

import { openDaemonEventStream } from "$lib/daemon/daemonEventStream";

describe("authenticated daemon event streams", () => {
  beforeEach(() => {
    mocks.listeners.clear();
    mocks.order.length = 0;
    mocks.start.mockClear();
    mocks.cancel.mockClear();
  });

  it("subscribes before starting and never opens a browser EventSource in Tauri", async () => {
    const received: Array<{ work_id: string }> = [];
    const connection = await openDaemonEventStream<{ work_id: string }>({
      operation: "forge.stream.get",
      browserUrl: async () => {
        throw new Error("browser EventSource must not be used");
      },
      browserEvent: "forge",
      onEvent: (event) => received.push(event),
      onError: () => {},
    });

    expect(mocks.order.slice(-3)).toEqual(["listen:event", "listen:error", "start"]);
    const [, , , handle] = mocks.start.mock.calls[0]!;
    expect(handle).toMatch(/^forge\.stream\.get-/);
    mocks.listeners.get(`daemon-stream://${handle}/event`)?.({
      payload: { work_id: "work-1" },
    });
    expect(received).toEqual([{ work_id: "work-1" }]);

    connection.close();
    expect(mocks.cancel).toHaveBeenCalledWith(handle);
    expect(connection.closed).toBe(true);
  });

  it("cancels and releases a failed native stream", async () => {
    const errors: string[] = [];
    const connection = await openDaemonEventStream<{ seq: number }>({
      operation: "forge.items.by_work_id.project_events.get",
      pathParams: { work_id: "work-1" },
      query: { since: "7" },
      browserUrl: async () => "http://127.0.0.1/unused",
      browserEvent: "project",
      onEvent: () => {},
      onError: (error) => errors.push(error.message),
    });
    const [, , , handle] = mocks.start.mock.calls[0]!;
    mocks.listeners.get(`daemon-stream://${handle}/error`)?.({
      payload: { message: "HTTP 401", recoverable: false },
    });

    expect(errors).toEqual(["HTTP 401"]);
    expect(connection.closed).toBe(true);
    expect(mocks.cancel).toHaveBeenCalledWith(handle);
    expect(mocks.listeners.size).toBe(0);
  });
});
