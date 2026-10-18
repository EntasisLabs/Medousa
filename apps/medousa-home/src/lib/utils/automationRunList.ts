import { cronToFriendly, describeFriendlySchedule } from "$lib/utils/friendlySchedule";
import type { GraphemeScriptEntry } from "$lib/types/grapheme";
import type { RecurringDefinitionEntry } from "$lib/types/recurring";
import type { WorkflowDetailResponse, WorkflowListEntry, WorkflowRunRequest } from "$lib/types/workflow";

export type AutomationFilter = "all" | "scheduled" | "manual" | "paused";
export interface AutomationRow {
  id: string;
  title: string;
  kind: "Prompt" | "Script" | "Flow";
  schedule?: RecurringDefinitionEntry;
  script?: GraphemeScriptEntry;
  flow?: WorkflowListEntry;
  lastStatus: string;
  lastAt: string | null;
}

/** A scheduled flow is one saved automation, rather than a schedule plus a duplicate flow. */
export function automationRows(schedules: RecurringDefinitionEntry[], scripts: GraphemeScriptEntry[], flows: WorkflowListEntry[]): AutomationRow[] {
  const linked = new Map<string, WorkflowListEntry>();
  for (const flow of flows) {
    if (!flow.scheduled_recurring_id) continue;
    const previous = linked.get(flow.scheduled_recurring_id);
    if (!previous || previous.created_at_utc < flow.created_at_utc) linked.set(flow.scheduled_recurring_id, flow);
  }
  const scheduleIds = new Set(schedules.map((entry) => entry.recurring_id));
  return [
    ...schedules.map((schedule): AutomationRow => ({
      id: `schedule:${schedule.recurring_id}`,
      title: schedule.display_name?.trim() || linked.get(schedule.recurring_id)?.name?.trim() || schedule.prompt_excerpt?.trim().split("\n")[0].slice(0, 100) || "Untitled schedule",
      kind: linked.has(schedule.recurring_id) ? "Flow" : "Prompt",
      schedule, flow: linked.get(schedule.recurring_id),
      lastStatus: schedule.last_run_status || (schedule.last_run_at_utc ? "Run recorded" : "Not run yet"), lastAt: schedule.last_run_at_utc,
    })),
    ...scripts.map((script): AutomationRow => ({ id: `script:${script.id}`, title: script.name || "Untitled script", kind: "Script", script, lastStatus: "—", lastAt: null })),
    ...flows.filter((flow) => !flow.scheduled_recurring_id || !scheduleIds.has(flow.scheduled_recurring_id)).map((flow): AutomationRow => ({
      id: `flow:${flow.workflow_id}`, title: flow.name?.trim() || "Untitled flow", kind: "Flow", flow,
      lastStatus: flow.status, lastAt: flow.created_at_utc,
    })),
  ].sort((a, b) => a.title.localeCompare(b.title));
}

export function filterAutomationRows(rows: AutomationRow[], filter: AutomationFilter, query: string): AutomationRow[] {
  const needle = query.trim().toLowerCase();
  return rows.filter((row) => (filter === "all" || (filter === "manual" ? !row.schedule && !row.flow?.scheduled_recurring_id : filter === "paused" ? row.schedule?.enabled === false : Boolean(row.schedule || row.flow?.scheduled_recurring_id))) &&
    (!needle || [row.title, row.kind, row.schedule?.prompt_excerpt, row.script?.intent, ...(row.script?.tags ?? [])].join(" ").toLowerCase().includes(needle)));
}

export function workflowReplayRequest(detail: WorkflowDetailResponse): WorkflowRunRequest {
  return { name: detail.name, strategy: detail.strategy, mode: detail.mode, on_failure: detail.on_failure, note: detail.note, steps: detail.steps };
}

/** Display cron without rewriting the saved schedule or exposing its syntax in the list. */
export function describeAutomationSchedule(cron: string, timezone: string): string {
  const parts = cron.trim().split(/\s+/);
  const custom = `Custom schedule${timezone ? ` (${timezone})` : ""}`;
  if (![5, 6, 7].includes(parts.length)) return custom;
  const offset = parts.length === 5 ? 0 : 1;
  if (!/^\d+$/.test(parts[offset]) || !/^\d+$/.test(parts[offset + 1])) return custom;
  if (offset && parts[0] !== "0" || parts.length === 7 && parts[6] !== "*") return custom;
  const dayIndex = offset + 4;
  const alias = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"].indexOf(parts[dayIndex].toUpperCase());
  if (alias >= 0) parts[dayIndex] = String(alias);
  const state = cronToFriendly(parts.join(" "));
  return state.frequency === "custom" ? custom : describeFriendlySchedule(state, timezone);
}
