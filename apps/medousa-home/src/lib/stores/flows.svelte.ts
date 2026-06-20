import {
  getWorkflow,
  listWorkflowRuns,
  listWorkflows,
  planWorkflow,
  promoteWorkflowFromSlice,
  runWorkflow,
  scheduleWorkflow,
} from "$lib/daemon";
import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
import type {
  FlowComposerDraft,
  WorkflowDetailResponse,
  WorkflowListEntry,
  WorkflowPlanResponse,
  WorkflowRunRequest,
  WorkflowScheduleRequest,
} from "$lib/types/workflow";
import {
  emptyFlowDraft,
  workflowRunRequestFromDraft,
} from "$lib/types/workflow";
import type { RecurringRunEntry } from "$lib/types/recurring";
import type { AutomationDeliveryMode } from "$lib/types/recurring";
import type { ToolHistorySliceRef } from "$lib/types/toolHistory";

export class FlowsStore {
  workflows = $state<WorkflowListEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  actionMessage = $state<string | null>(null);
  running = $state(false);
  planning = $state(false);
  scheduling = $state(false);
  detailById = $state<Record<string, WorkflowDetailResponse>>({});
  detailLoadingId = $state<string | null>(null);
  detailErrorById = $state<Record<string, string>>({});
  runsById = $state<Record<string, RecurringRunEntry[]>>({});
  runsLoadingId = $state<string | null>(null);
  runsErrorById = $state<Record<string, string>>({});
  composerOpen = $state(false);
  composerDraft = $state<FlowComposerDraft>(emptyFlowDraft());
  lastPlan = $state<WorkflowPlanResponse | null>(null);

  async refresh(limit = 50) {
    this.loading = true;
    try {
      const response = await listWorkflows(limit);
      this.workflows = response.workflows;
      this.error = null;
    } catch (err) {
      if (this.workflows.length === 0) {
        this.error = formatDaemonErrorSummary(err);
      }
    } finally {
      this.loading = false;
    }
  }

  async loadDetail(workflowId: string) {
    this.detailLoadingId = workflowId;
    try {
      const detail = await getWorkflow(workflowId);
      this.detailById = { ...this.detailById, [workflowId]: detail };
      const { [workflowId]: _removed, ...rest } = this.detailErrorById;
      this.detailErrorById = rest;
    } catch (err) {
      this.detailErrorById = {
        ...this.detailErrorById,
        [workflowId]: err instanceof Error ? err.message : String(err),
      };
    } finally {
      this.detailLoadingId = null;
    }
  }

  async loadRuns(workflowId: string, limit = 20) {
    this.runsLoadingId = workflowId;
    try {
      const response = await listWorkflowRuns(workflowId, limit);
      this.runsById = { ...this.runsById, [workflowId]: response.runs };
      const { [workflowId]: _removed, ...rest } = this.runsErrorById;
      this.runsErrorById = rest;
    } catch (err) {
      this.runsErrorById = {
        ...this.runsErrorById,
        [workflowId]: err instanceof Error ? err.message : String(err),
      };
    } finally {
      this.runsLoadingId = null;
    }
  }

  openComposer(seed?: Partial<FlowComposerDraft>) {
    this.composerDraft = { ...emptyFlowDraft(), ...seed };
    this.lastPlan = null;
    this.composerOpen = true;
    this.actionMessage = null;
  }

  closeComposer() {
    this.composerOpen = false;
    this.composerDraft = emptyFlowDraft();
    this.lastPlan = null;
  }

  labelFor(entry: WorkflowListEntry): string {
    if (entry.name?.trim()) return entry.name.trim();
    return entry.workflow_id;
  }

  statusLabel(status: string | null | undefined): string {
    if (!status) return "—";
    if (status === "succeeded") return "Succeeded";
    if (status === "failed") return "Failed";
    if (status === "running") return "Running";
    if (status === "enqueued") return "Enqueued";
    if (status === "canceled") return "Canceled";
    return status;
  }

  formatTimestamp(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  buildDeliveryPayload(
    mode: AutomationDeliveryMode,
    telegramChatId?: string,
  ): Record<string, unknown> | null {
    if (mode === "in_app" || mode === "quiet") return null;
    if (mode === "telegram") {
      const chatId = telegramChatId?.trim();
      if (!chatId) return null;
      return {
        channel: "telegram",
        telegram_chat_id: chatId,
      };
    }
    return null;
  }

  async planFromGoal(goal: string) {
    this.planning = true;
    this.actionMessage = null;
    try {
      const response = await planWorkflow({ goal: goal.trim() });
      this.lastPlan = response;
      if (response.suggested_workflow?.steps?.length) {
        this.composerDraft = {
          ...this.composerDraft,
          name: response.suggested_workflow.name?.trim() ?? this.composerDraft.name,
          steps: response.suggested_workflow.steps,
        };
      }
      if (response.suggested_schedule) {
        this.composerDraft = {
          ...this.composerDraft,
          cron_expr: response.suggested_schedule.cron_expr,
          timezone: response.suggested_schedule.timezone,
        };
      }
      this.actionMessage = `Plan ready · ${response.confidence} confidence · ${response.execute_with}`;
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.planning = false;
    }
  }

  async runDraft(draft: FlowComposerDraft) {
    if (draft.steps.length === 0) {
      throw new Error("Add at least one step before running.");
    }
    this.running = true;
    this.actionMessage = null;
    try {
      const request = workflowRunRequestFromDraft(draft);
      const response = await runWorkflow(request);
      this.actionMessage = `Flow enqueued · ${response.workflow_id}`;
      this.closeComposer();
      await this.refresh();
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.running = false;
    }
  }

  async scheduleDraft(
    draft: FlowComposerDraft,
    deliveryMode: AutomationDeliveryMode = "in_app",
    telegramChatId?: string,
  ) {
    if (draft.steps.length === 0) {
      throw new Error("Add at least one step before scheduling.");
    }
    this.scheduling = true;
    this.actionMessage = null;
    try {
      const base = workflowRunRequestFromDraft(draft);
      const request: WorkflowScheduleRequest = {
        ...base,
        cron_expr: draft.cron_expr.trim() || "0 9 * * *",
        timezone: draft.timezone.trim() || "UTC",
        display_name: draft.name.trim() || null,
        delivery: this.buildDeliveryPayload(deliveryMode, telegramChatId),
        enabled: true,
      };
      const response = await scheduleWorkflow(request);
      this.actionMessage = `Scheduled · next ${this.formatTimestamp(response.next_run_at_utc)}`;
      this.closeComposer();
      await this.refresh();
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.scheduling = false;
    }
  }

  async applyFromSliceRefs(refs: ToolHistorySliceRef[], name?: string) {
    this.actionMessage = null;
    const response = await promoteWorkflowFromSlice({ refs, name: name ?? null, run: false });
    this.composerDraft = {
      ...emptyFlowDraft(),
      name: response.draft.name ?? "",
      steps: response.draft.steps,
    };
    this.lastPlan = null;
    this.composerOpen = true;
    if (response.notes.length > 0) {
      this.actionMessage = response.notes.join(" · ");
    }
    return response;
  }

  async rerun(request: WorkflowRunRequest) {
    this.running = true;
    this.actionMessage = null;
    try {
      const response = await runWorkflow(request);
      this.actionMessage = `Flow enqueued · ${response.workflow_id}`;
      await this.refresh();
      return response;
    } catch (err) {
      this.actionMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.running = false;
    }
  }
}

export const flows = new FlowsStore();
