import { describe, expect, it, vi } from "vitest";
const { call } = vi.hoisted(() => ({ call: vi.fn().mockResolvedValue({}) }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: call }));
import { creativeApi } from "./creativeApi";

describe("creative job workshop binding", () => {
  it("preserves job identity and uses the original workshop for every operation", async () => {
    call.mockClear();
    const api = creativeApi(" workshop-one ");
    const request = { id: "job-one", expected_revision: 4, selected_ids: ["frame"], prompt: "Refine this" };
    await api.createJob("doc", request);
    await api.jobs("doc");
    await api.job("doc", "job-one");
    await api.decideJob("doc", "job-one", { accept: false, expected_revision: 4 });
    expect(call.mock.calls).toEqual([
      ["create.projects.by_id.jobs.post", { id: "doc" }, request, "workshop-one"],
      ["create.projects.by_id.jobs.get", { id: "doc" }, undefined, "workshop-one"],
      ["create.projects.by_id.jobs.by_job_id.get", { id: "doc", job_id: "job-one" }, undefined, "workshop-one"],
      ["create.projects.by_id.jobs.by_job_id.decision.post", { id: "doc", job_id: "job-one" }, { accept: false, expected_revision: 4 }, "workshop-one"],
    ]);
  });
});
