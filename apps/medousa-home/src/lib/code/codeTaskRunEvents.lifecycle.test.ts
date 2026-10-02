import { afterEach, expect, it, vi } from "vitest";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
const open = vi.hoisted(() => vi.fn());
vi.mock("$lib/daemon/daemonEventStream", () => ({ openDaemonEventStream: open }));
import { CodeTaskRunEventStream } from "./codeTaskRunEvents";

afterEach(() => { setActiveWorkshopIdPort(null); open.mockReset(); });

it("ignores callbacks and closes a late connection after restarting the same run", async () => {
  let resolve!: (value: unknown) => void;
  open.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  open.mockResolvedValue({ closed: false, close: vi.fn() });
  const onEvent = vi.fn();
  const stream = new CodeTaskRunEventStream({ onEvent });
  stream.start("same-id", "same-run");
  const stale = open.mock.calls[0]![0];
  stream.start("same-id", "same-run");
  const close = vi.fn();
  resolve({ closed: false, close });
  await Promise.resolve();
  stale.onEvent({ seq: 100, run_id: "same-run", kind: "output", text: "old" });
  stale.onOpen();
  stale.onError();
  expect(close).toHaveBeenCalledOnce();
  expect(onEvent).not.toHaveBeenCalled();
  expect(stream.cursor).toBe(0);
  stream.teardown();
});

it("ignores output after a workshop change when both use the parent transport", async () => {
  let workshop = "workshop-a";
  setActiveWorkshopIdPort(() => workshop);
  open.mockResolvedValue({ closed: false, close: vi.fn() });
  const onEvent = vi.fn();
  const stream = new CodeTaskRunEventStream({ onEvent });
  stream.start("same-id", "same-run");
  await Promise.resolve();
  workshop = "workshop-b";
  open.mock.calls[0]![0].onEvent({ seq: 1, run_id: "same-run", kind: "output", text: "old" });
  expect(onEvent).not.toHaveBeenCalled();
  expect(stream.cursor).toBe(0);
  stream.teardown();
});
