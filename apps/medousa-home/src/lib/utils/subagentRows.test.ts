import { describe, expect, it } from "vitest";
import type { WorkerToolActivity } from "$lib/types/card";
import {
  subagentRowsForSession,
  toolRunsFromWorkerActivity,
} from "$lib/utils/subagentRows";

function activity(overrides: Partial<WorkerToolActivity> = {}): WorkerToolActivity {
  return {
    run_id: "run-1",
    name: "web_search",
    round: 1,
    status: "succeeded",
    input_summary: "newest Qwen models",
    input_params: [{ key: "query", value: "newest Qwen models", truncated: false }],
    output_summary: "6 results",
    started_at: "2026-08-07T00:00:01Z",
    finished_at: "2026-08-07T00:00:03Z",
    ...overrides,
  };
}

describe("toolRunsFromWorkerActivity", () => {
  it("maps a worker run onto the shared ToolRunState shape", () => {
    const [run] = toolRunsFromWorkerActivity([activity()]);
    expect(run.runId).toBe("run-1");
    expect(run.toolName).toBe("web_search");
    expect(run.round).toBe(1);
    expect(run.status).toBe("succeeded");
    expect(run.outputSummary).toBe("6 results");
  });

  it("carries redacted arguments through so chat can show what was searched", () => {
    const [run] = toolRunsFromWorkerActivity([activity()]);
    expect(run.inputParams).toEqual([
      { key: "query", value: "newest Qwen models", truncated: false },
    ]);
  });

  it("normalizes worker statuses onto the three render states", () => {
    const runs = toolRunsFromWorkerActivity([
      activity({ run_id: "a", status: "running", finished_at: null }),
      activity({ run_id: "b", status: "failed" }),
      // Anything unrecognized settles rather than spinning forever.
      activity({ run_id: "c", status: "finished" }),
    ]);
    expect(runs.map((run) => run.status)).toEqual(["running", "failed", "succeeded"]);
  });

  it("tolerates runs with no recorded arguments", () => {
    const [run] = toolRunsFromWorkerActivity([
      activity({ input_params: undefined, input_summary: null }),
    ]);
    expect(run.inputParams).toBeUndefined();
    expect(run.inputSummary).toBeNull();
  });
});

describe("subagentRowsForSession", () => {
  it("returns no rows for a session with no workers", () => {
    expect(subagentRowsForSession("nonexistent-session")).toEqual([]);
  });
});
