import { describe, it, expect, vi, afterEach } from "vitest";
import { creativeJobs } from "./creativeJobs";
import type { CreativeApi } from "./creativeApi";
import type { CreativeJob } from "$lib/types/generated/daemon_api";
const job = (status: CreativeJob["status"] = "running"): CreativeJob => ({
  document_id: "doc",
  request: {
    id: "job",
    expected_revision: 0,
    selected_ids: ["frame"],
    prompt: "Make it green",
  },
  status,
  session_id: "session",
  turn_id: "turn",
  created_at: "now",
  updated_at: "now",
  assets: [],
  context: {
    document_id: "doc",
    revision: 0,
    selected_ids: ["frame"],
    nodes: [],
    assets: [],
    fingerprint: "x",
  },
});
function api() {
  return {
    createJob: vi.fn().mockResolvedValue(job()),
    jobs: vi.fn().mockResolvedValue({ jobs: [] }),
    job: vi.fn().mockResolvedValue(job()),
    decideJob: vi.fn().mockResolvedValue(job("accepted")),
  } as unknown as CreativeApi;
}
afterEach(() => vi.useRealTimers());
describe("creative job UI ownership", () => {
  it("retries an uncertain admission with the identical identity and captured selection", async () => {
    const port = api();
    vi.mocked(port.createJob).mockRejectedValueOnce(new Error("lost response"));
    const jobs = creativeJobs(port, "doc", vi.fn());
    const selected = ["frame"];
    await expect(jobs.start("Make it green", 0, selected)).rejects.toThrow(
      "lost response",
    );
    selected.push("other");
    expect(jobs.pending).toBe(true);
    await jobs.retry();
    expect(vi.mocked(port.createJob).mock.calls[0]).toEqual(
      vi.mocked(port.createJob).mock.calls[1],
    );
    expect(jobs.job?.request.selected_ids).toEqual(["frame"]);
    expect(jobs.job).not.toHaveProperty("context");
  });
  it("retains a lost acceptance receipt and cannot replace it with cancellation", async () => {
    const port = api(),
      accepted = vi.fn();
    vi.mocked(port.decideJob).mockRejectedValueOnce(
      new Error("lost acknowledgement"),
    );
    const jobs = creativeJobs(port, "doc", accepted);
    await jobs.start("Green", 0, ["frame"]);
    await expect(jobs.decide(true, 0)).rejects.toThrow("lost acknowledgement");
    await expect(jobs.decide(false, 1)).rejects.toThrow("pending decision");
    await jobs.decide(true, 99);
    expect(vi.mocked(port.decideJob).mock.calls[0]).toEqual(
      vi.mocked(port.decideJob).mock.calls[1],
    );
    expect(accepted).toHaveBeenCalledOnce();
  });
  it("polls only while a view is present and stops on terminal outcomes", async () => {
    vi.useFakeTimers();
    const port = api(),
      jobs = creativeJobs(port, "doc", vi.fn());
    await jobs.start("Green", 0, ["frame"]);
    const stop = jobs.retain();
    await Promise.resolve();
    await Promise.resolve();
    vi.mocked(port.job).mockResolvedValue(job("ready"));
    await vi.advanceTimersByTimeAsync(2000);
    const n = vi.mocked(port.job).mock.calls.length;
    await vi.advanceTimersByTimeAsync(12000);
    expect(vi.mocked(port.job).mock.calls).toHaveLength(n);
    stop();
    await vi.advanceTimersByTimeAsync(12000);
    expect(vi.mocked(port.job).mock.calls).toHaveLength(n);
  });
  it("does not let a slow initial list replace a newly admitted job", async () => {
    const port = api();
    let finish!: (value: { jobs: CreativeJob[] }) => void;
    vi.mocked(port.jobs).mockImplementationOnce(
      () => new Promise((resolve) => (finish = resolve)),
    );
    const jobs = creativeJobs(port, "doc", vi.fn()),
      stop = jobs.retain();
    await jobs.start("Green", 0, ["frame"]);
    finish({
      jobs: [{ ...job("failed"), request: { ...job().request, id: "older" } }],
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(jobs.job?.request.id).toBe("job");
    stop();
  });
});
it("keeps oversized Unicode requests editable instead of pinning an invalid retry", async () => {
  const port = api(),
    jobs = creativeJobs(port, "doc", vi.fn());
  await expect(jobs.start("🌮".repeat(5000), 0, ["frame"])).rejects.toThrow(
    "16 KiB",
  );
  expect(jobs.pending).toBe(false);
  expect(port.createJob).not.toHaveBeenCalled();
});
it("restores the current job when a view reopens during its first read", async () => {
  const port = api();
  let finish!: (value: { jobs: CreativeJob[] }) => void;
  vi.mocked(port.jobs)
    .mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)))
    .mockResolvedValue({ jobs: [job("ready")] });
  const jobs = creativeJobs(port, "doc", vi.fn()),
    first = jobs.retain();
  first();
  const second = jobs.retain();
  finish({ jobs: [job("running")] });
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  expect(port.jobs).toHaveBeenCalledTimes(2);
  expect(jobs.job?.status).toBe("ready");
  second();
});
