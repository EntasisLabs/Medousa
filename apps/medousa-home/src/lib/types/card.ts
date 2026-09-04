import type { WorkCard } from "$lib/types/workspace";

export interface WorkCardAssociations {
  vault_paths: string[];
  artifact_ids: string[];
  locus_node_ids: string[];
}

/** One redacted argument of a tool call, e.g. `query: "svelte 5 runes"`. */
export type { ToolInputParam } from "$lib/types/generated/daemon_api";
import type { ToolInputParam } from "$lib/types/generated/daemon_api";

/** One worker tool run, correlated start-to-finish by `run_id`. */
export interface WorkerToolActivity {
  run_id: string;
  name: string;
  round: number;
  /** running | succeeded | failed */
  status: string;
  input_summary?: string | null;
  input_params?: ToolInputParam[];
  output_summary?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface WorkCardDetail {
  card: WorkCard;
  kind: string;
  subtitle?: string | null;
  session_id?: string | null;
  correlation_id?: string | null;
  manuscript_id?: string | null;
  job_id?: string | null;
  work_id?: string | null;
  job_type?: string | null;
  user_ack?: string | null;
  wrapping_up_reasons: string[];
  terminal: boolean;
  error?: string | null;
  result_excerpt?: string | null;
  task_line?: string | null;
  tool_names?: string[] | null;
  associations: WorkCardAssociations;
  /** Live worker tool runs, correlated start-to-finish by `run_id`. */
  live_tool_activity?: WorkerToolActivity[];
  /** Rolling worker reasoning transcript (joined chunks, capped tail). */
  live_thinking?: string;
  /** Rolling worker assistant output preview (joined chunks, capped tail). */
  live_output?: string;
  /** First/last reasoning chunk, so chat can render "Thought for Ns". */
  thinking_started_at?: string | null;
  thinking_finished_at?: string | null;
  /** Live status line (Running tool X / Thinking… / Synthesizing…). */
  live_status_line?: string | null;
  /** Worker model/provider whisper. */
  model?: string | null;
  /** Stable runtime identity that actually executed this worker. */
  execution_runtime_id?: string | null;
}

/**
 * Live transcript slice pushed alongside `card_upserted` for turn-worker cards,
 * so chat ticks without a detail round trip.
 */
export interface WorkerProgress {
  work_id: string;
  session_id?: string | null;
  execution_runtime_id?: string | null;
  live_tool_activity?: WorkerToolActivity[];
  live_thinking?: string;
  live_output?: string;
  thinking_started_at?: string | null;
  thinking_finished_at?: string | null;
  live_status_line?: string | null;
  model?: string | null;
  result_excerpt?: string | null;
  terminal: boolean;
  column: string;
}
