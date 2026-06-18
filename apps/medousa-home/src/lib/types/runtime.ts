export interface DaemonStatsResponse {
  enqueued_jobs: number;
  running_jobs: number;
  succeeded_jobs: number;
  failed_jobs: number;
  dead_letter_jobs: number;
  pending_outbox_events: number;
  recurring_definitions: number;
  last_tick_at_utc: string | null;
}

export interface DeliveryHealthResponse {
  endpoint_id: string;
  endpoint_seeded: boolean;
  endpoint_target: string;
  deliver_webhook_auth_configured: boolean;
  pending_job_deliveries: number;
  last_delivery_at_utc: string | null;
  last_delivery_latency_ms: number | null;
}

export interface ContinuationStatusResponse {
  pending_count: number;
  consumed_count: number;
  resumed_count: number;
  dead_letter_pending_count: number;
  total_count: number;
  last_resume_at_utc: string | null;
  last_resume_child_job_id: string | null;
  last_resume_turn_correlation_id: string | null;
}

export type DepthMode = "concise" | "standard" | "deep";

export type ReasoningEffortMode = import("./reasoningEffort").ReasoningEffortMode;

export interface StageRoute {
  role: string;
  provider: string;
  model: string;
  policy_profile: string;
  fallback_chain: string[];
}

export interface StageRoutingMatrix {
  orchestrator: StageRoute;
  chunker: StageRoute;
  extractor: StageRoute;
  summarizer: StageRoute;
  verifier: StageRoute;
  packer: StageRoute;
  final_response: StageRoute;
}

export interface RuntimeConfigCommandResponse {
  rendered_output: string | null;
  next_draft_provider: string;
  next_draft_model: string;
  next_response_depth_mode: string;
  next_reasoning_effort: string;
  should_apply_settings: boolean;
  should_persist_depth_defaults: boolean;
  should_persist_reasoning_defaults: boolean;
}

export interface RuntimeDefaultsResponse {
  backend: string;
  provider: string;
  model: string;
  response_depth_mode: string;
  reasoning_effort: string;
  base_url: string | null;
  stage_routing: StageRoutingMatrix;
  work_card_hide_after_hours: number;
  work_card_wipe_after_days: number;
  active_profile_id?: string;
  active_profile_display_name?: string;
}

export interface StageRouteCommandResponse {
  stage_routing: StageRoutingMatrix;
  rendered_output: string;
}

export type RuntimeTab =
  | "now"
  | "jobs"
  | "schedule"
  | "delivery"
  | "controls"
  | "workshop"
  | "routing";
