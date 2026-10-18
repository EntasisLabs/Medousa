import { invoke } from "@tauri-apps/api/core";
import { getWorkspaceCard, listToolHistorySlices, listRecurring, listRecurringRuns, operationPath, type OperationId } from "$lib/daemon";
import { forgeFetch, type ItemProjection, type ProjectTaskRun, type ProjectTaskRunList } from "$lib/forge";
import { isTauri } from "$lib/platform";
import type { ActivityDataPort } from "./activityData.svelte";

/** Global Activity reads the connected workshop, not a Code tab's executor override. */
async function readForge<T>(operation: OperationId, params: Record<string, string> = {}, query = ""): Promise<T> {
  const path = operationPath(operation, params) + query;
  if (isTauri()) return invoke<T>("forge_request", { method: "GET", path, body: null, executionRuntimeId: null });
  return forgeFetch<T>(path);
}

export const activityDataPort: ActivityDataPort = {
  async tools() { return (await listToolHistorySlices({ limit: 120 })).runs; },
  async projects(cursor) {
    const query = new URLSearchParams({ limit: "128" });
    if (cursor) query.set("cursor", cursor);
    const response = await readForge<ItemProjection[] | { items: ItemProjection[]; next_cursor?: string; truncated?: boolean }>("forge.items.get", {}, `?${query}`);
    return Array.isArray(response) ? { items: response, nextCursor: null } : { items: response.items, nextCursor: response.truncated ? response.next_cursor || null : null };
  },
  async projectRuns(workId) {
    return (await readForge<ProjectTaskRunList>("forge.items.by_work_id.task_runs.get", { work_id: workId }, "?limit=20")).runs;
  },
  async schedules() { return (await listRecurring(false)).recurring; },
  async scheduleRuns(id) { return (await listRecurringRuns(id, 20)).runs; },
  card: getWorkspaceCard,
};

export function readActivityCodeRun(workId: string, runId: string): Promise<ProjectTaskRun> {
  return readForge("forge.items.by_work_id.task_runs.by_run_id.get", { work_id: workId, run_id: runId });
}
