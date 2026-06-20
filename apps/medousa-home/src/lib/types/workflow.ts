import type { RecurringRunEntry } from "$lib/types/recurring";

export type WorkflowStepKind = "grapheme" | "prompt" | "mcp";

export interface WorkflowStepGrapheme {
  kind: "grapheme";
  id: string;
  source: string;
}

export interface WorkflowStepPrompt {
  kind: "prompt";
  id: string;
  user_prompt: string;
  system_prompt?: string | null;
}

export interface WorkflowStepMcp {
  kind: "mcp";
  id: string;
  server_id: string;
  tool_name: string;
  args?: Record<string, unknown>;
  effect_class?: string | null;
}

export type WorkflowStepSpec =
  | WorkflowStepGrapheme
  | WorkflowStepPrompt
  | WorkflowStepMcp;

export interface WorkflowRunRequest {
  name?: string | null;
  strategy?: string;
  mode?: string;
  steps: WorkflowStepSpec[];
  on_failure?: string;
  note?: string | null;
  queue?: string | null;
}

export interface WorkflowListEntry {
  workflow_id: string;
  name?: string | null;
  status: string;
  strategy: string;
  mode: string;
  root_job_id: string;
  root_job_state?: string | null;
  scheduled_recurring_id?: string | null;
  created_at_utc: string;
  step_count: number;
}

export interface WorkflowsListResponse {
  count: number;
  workflows: WorkflowListEntry[];
}

export interface WorkflowStepResultDto {
  id: string;
  kind: string;
  status: string;
  output?: unknown;
  error?: string | null;
}

export interface WorkflowDetailResponse {
  workflow_id: string;
  name?: string | null;
  status: string;
  strategy: string;
  mode: string;
  on_failure: string;
  note?: string | null;
  root_job_id: string;
  root_job_state?: string | null;
  scheduled_recurring_id?: string | null;
  created_at_utc: string;
  steps: WorkflowStepSpec[];
  step_results: WorkflowStepResultDto[];
}

export interface WorkflowRunResponse {
  workflow_id: string;
  status: string;
  strategy: string;
  root_job_id: string;
  job_type: string;
  lane: string;
}

export interface WorkflowPlanRequest {
  goal: string;
  context?: Record<string, unknown> | null;
}

export interface WorkflowScheduleSuggestion {
  cron_expr: string;
  timezone: string;
}

export interface WorkflowPlanResponse {
  goal: string;
  confidence: string;
  execute_with: string;
  suggested_workflow?: WorkflowRunRequest | null;
  suggested_schedule?: WorkflowScheduleSuggestion | null;
  suggested_tool_input?: unknown;
  notes: string[];
  assumptions: string[];
}

export interface WorkflowScheduleRequest extends WorkflowRunRequest {
  cron_expr: string;
  timezone?: string;
  display_name?: string | null;
  recurring_id?: string | null;
  delivery?: Record<string, unknown> | null;
  enabled?: boolean;
}

export interface WorkflowScheduleResponse {
  workflow_id: string;
  status: string;
  recurring_id: string;
  cron_expr: string;
  timezone: string;
  next_run_at_utc: string;
  materialized_job_id?: string | null;
}

export interface WorkflowRunsResponse {
  workflow_id: string;
  count: number;
  runs: RecurringRunEntry[];
}

export interface FlowComposerDraft {
  name: string;
  goal: string;
  steps: WorkflowStepSpec[];
  cron_expr: string;
  timezone: string;
}

export function newStepId(prefix: string): string {
  return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
}

export function emptyFlowDraft(): FlowComposerDraft {
  return {
    name: "",
    goal: "",
    steps: [],
    cron_expr: "0 9 * * *",
    timezone: "UTC",
  };
}

export function workflowRunRequestFromDraft(
  draft: FlowComposerDraft,
): WorkflowRunRequest {
  return {
    name: draft.name.trim() || null,
    strategy: "sequential",
    mode: "default",
    on_failure: "stop",
    steps: draft.steps,
    queue: "default",
  };
}
