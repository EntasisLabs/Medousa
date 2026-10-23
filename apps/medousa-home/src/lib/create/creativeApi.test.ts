import { describe, expect, it, vi } from "vitest";
const { call } = vi.hoisted(() => ({ call: vi.fn().mockResolvedValue({}) }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: call }));
import { creativeApi } from "./creativeApi";

describe("creative job workshop binding", () => {
  it("preserves job identity and uses the original workshop for every operation", async () => {
    call.mockClear();
    const api = creativeApi(" workshop-one ");
    const request = {
      id: "job-one",
      expected_revision: 4,
      selected_ids: ["frame"],
      prompt: "Refine this",
    };
    await api.createJob("doc", request);
    await api.jobs("doc");
    await api.job("doc", "job-one");
    await api.decideJob("doc", "job-one", {
      accept: false,
      expected_revision: 4,
    });
    expect(call.mock.calls).toEqual([
      [
        "create.projects.by_id.jobs.post",
        { id: "doc" },
        request,
        null,
        undefined,
        "workshop-one",
      ],
      [
        "create.projects.by_id.jobs.get",
        { id: "doc" },
        undefined,
        null,
        undefined,
        "workshop-one",
      ],
      [
        "create.projects.by_id.jobs.by_job_id.get",
        { id: "doc", job_id: "job-one" },
        undefined,
        null,
        undefined,
        "workshop-one",
      ],
      [
        "create.projects.by_id.jobs.by_job_id.decision.post",
        { id: "doc", job_id: "job-one" },
        { accept: false, expected_revision: 4 },
        null,
        undefined,
        "workshop-one",
      ],
    ]);
  });
});

const overloaded = 'workshop returned HTTP 503 Service Unavailable: {"schema_version":1,"code":"unavailable_feature","message":"execution overloaded: forge queued-byte budget is exhausted (limit 8388608 bytes, this admission needs 8388608 bytes)","request_id":"unassigned","details":{}}';
it("recovers startup reads quietly and keeps the original workshop binding", async () => {
  vi.useFakeTimers();
  try {
    call.mockReset().mockRejectedValueOnce(new Error(overloaded)).mockResolvedValueOnce({ projects: [], next_cursor: null });
    const loading = creativeApi("original-workshop").list();
    await vi.advanceTimersByTimeAsync(150);
    await expect(loading).resolves.toEqual({ projects: [], next_cursor: null });
    expect(call).toHaveBeenCalledTimes(2);
    expect(call.mock.calls.every(c => c[5] === "original-workshop")).toBe(true);
  } finally { vi.useRealTimers(); call.mockReset().mockResolvedValue({}); }
});
it("bounds retries and presents useful copy instead of transport JSON", async () => {
  vi.useFakeTimers();
  try {
    call.mockReset().mockRejectedValue(new Error(overloaded));
    const result = expect(creativeApi("one").get("doc")).rejects.toThrow("workshop is busy");
    await vi.runAllTimersAsync();
    await result;
    expect(call).toHaveBeenCalledTimes(4);
  } finally { vi.useRealTimers(); call.mockReset().mockResolvedValue({}); }
});
it("does not automatically resend writes or retry validation failures", async () => {
  call.mockReset().mockRejectedValue(new Error(overloaded));
  await expect(creativeApi("one").delete("doc", 1)).rejects.toThrow("workshop is busy");
  expect(call).toHaveBeenCalledTimes(1);
  call.mockReset().mockRejectedValue(new Error('workshop returned HTTP 400: {"message":"invalid creative document: missing parent","code":"invalid_parameter"}'));
  await expect(creativeApi("one").get("doc")).rejects.toThrow("invalid creative document: missing parent");
  expect(call).toHaveBeenCalledTimes(1);
  call.mockReset().mockResolvedValue({});
});
