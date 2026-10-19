import { describe, expect, it } from "vitest";
import { automationRows, describeAutomationSchedule, filterAutomationRows, workflowReplayRequest } from "./automationRunList";
import type { RecurringDefinitionEntry } from "$lib/types/recurring";
import type { GraphemeScriptEntry } from "$lib/types/grapheme";
import type { WorkflowDetailResponse, WorkflowListEntry } from "$lib/types/workflow";
const schedule = (id: string, enabled = true) => ({ recurring_id: id, enabled, display_name: `Schedule ${id}`, last_run_status: "failed", last_run_at_utc: "2026-10-17T10:00:00Z" }) as RecurringDefinitionEntry;
const script = { id: "same", name: "Backup", tags: ["disk"] } as GraphemeScriptEntry;
const flow = (id: string, recurringId?: string, created = "2026-10-17") => ({ workflow_id: id, name: `Flow ${id}`, scheduled_recurring_id: recurringId, created_at_utc: created, status: "succeeded" }) as WorkflowListEntry;

describe("saved automation rows", () => {
  it("joins scheduled flows by recurring identity and namespaces resource ids", () => {
    const rows = automationRows([schedule("same")], [script], [flow("old", "same", "2026-10-16"), flow("latest", "same"), flow("manual")]);
    expect(rows).toHaveLength(3);
    expect(new Set(rows.map((row) => row.id)).size).toBe(3);
    expect(rows.find((row) => row.id === "schedule:same")).toMatchObject({ kind: "Flow", flow: { workflow_id: "latest" }, lastStatus: "failed" });
  });
  it("keeps scheduled, paused and manual work separate and searches script metadata", () => {
    const rows = automationRows([schedule("on"), schedule("off", false)], [script], [flow("manual")]);
    expect(filterAutomationRows(rows, "paused", "").map((row) => row.id)).toEqual(["schedule:off"]);
    expect(filterAutomationRows(rows, "scheduled", "")).toHaveLength(2);
    expect(filterAutomationRows(rows, "manual", "").map((row) => row.id)).toEqual(["script:same", "flow:manual"]);
    expect(filterAutomationRows(rows, "all", "disk").map((row) => row.id)).toEqual(["script:same"]);
  });
  it("replays the actual flow definition without carrying an old execution identity", () => {
    const detail = { ...flow("run"), strategy: "sequential", mode: "default", on_failure: "stop", note: "Saved", steps: [{ id: "one", kind: "prompt", user_prompt: "Complete prompt" }], step_results: [], root_job_id: "previous-job" } as WorkflowDetailResponse;
    expect(workflowReplayRequest(detail)).toEqual({ name: "Flow run", strategy: "sequential", mode: "default", on_failure: "stop", note: "Saved", steps: detail.steps });
    expect(workflowReplayRequest(detail)).not.toHaveProperty("root_job_id");
  });
});


it("reads named weekdays without rewriting or displaying cron syntax", () => {
  expect(describeAutomationSchedule("0 0 3 * * Sun *", "UTC")).toBe("Every Sunday at 3:00 AM (UTC)");
  expect(describeAutomationSchedule("0 0 */4 * * * *", "UTC")).toBe("Custom schedule (UTC)");
});
