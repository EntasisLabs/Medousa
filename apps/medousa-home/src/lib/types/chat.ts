export interface ToolArtifactRef {
  role: string;
  content_type: string;
  byte_size: number;
  hash64: string;
  artifact_id?: string | null;
  label?: string | null;
}

export type UiArtifactPresentation = "inline" | "panel" | "fullscreen";

export interface UiArtifact {
  artifactId: string;
  mime: string;
  label: string;
  presentation: UiArtifactPresentation;
  byteSize?: number | null;
  heightPx?: number | null;
}

export interface ToolRunState {
  runId: string;
  toolName: string;
  status: "running" | "succeeded" | "failed";
  round: number;
  inputSummary?: string | null;
  outputSummary?: string | null;
  artifactRefs?: ToolArtifactRef[];
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  streaming?: boolean;
  /** Latest daemon turn phase whisper (e.g. tool_loop, synthesis). */
  phase?: string | null;
  /** Human status line from stream event.message. */
  statusLine?: string | null;
  /** Tools invoked this turn (cumulative). */
  tools?: string[];
  /** Structured tool runs grouped by round (P1). */
  toolRuns?: ToolRunState[];
  /** Rich HTML artifacts presented via cognition_ui_present. */
  uiArtifacts?: UiArtifact[];
  /** Collapsed reasoning scratch from reasoning_delta. */
  reasoning?: string | null;
  /** Verifier / turn lifecycle state from session history. */
  answerState?: string | null;
  /** Turn ticket that produced this bubble. */
  turnId?: string | null;
  /** Stage-direction whisper (e.g. worker handoff ack) above the main voice. */
  stageWhisper?: string | null;
  /** `ask` = background /ask lane; `worker` = delegated workshop lane; `chat` = principal conversation (default). */
  lane?: "chat" | "ask" | "worker" | null;
  /** Workspace card / job id for ask-lane messages. */
  askJobId?: string | null;
  /** Turn worker id for worker-lane messages. */
  workId?: string | null;
  /** Turn paused for operator tool-round budget approval. */
  budgetRequestId?: string | null;
  requestedRounds?: number | null;
  /** Turn ended with an error — partial content preserved for debugging. */
  failed?: boolean;
  errorLine?: string | null;
  /** Turn index in session history (1-based, matches slice_id turn:N). */
  turnIndex?: number | null;
}

export interface PendingBrowserChallenge {
  turnId: string;
  messageId: string | null;
  sessionId: string;
  challengeUrl: string | null;
  message: string;
}

export interface PendingBudgetApproval {
  turnId: string;
  messageId: string | null;
  /** Daemon budget request id (approve/deny API). */
  requestId: string;
  /** Workspace card id for navigation — same as requestId for turn.budget_request cards. */
  workCardId: string;
  requestedRounds: number | null;
  message: string;
}

export interface TurnTicketState {
  turnId: string;
  mode: "interactive" | "background";
  phase: string;
  messageId: string | null;
  streamAttached: boolean;
  terminal: boolean;
  workspaceCardId?: string | null;
  budgetRequestId?: string | null;
  requestedRounds?: number | null;
}

export interface InteractiveTurnStreamEvent {
  turn_id: string;
  /** Monotonic per-turn sequence stamped server-side; enables exactly-once replay/dedup. */
  /** @see $lib/types/generated/daemon_api.ts (schema-generated contract) */
  seq?: number;
  event_type: string;
  phase: string;
  message: string;
  content_delta?: string | null;
  reasoning_delta?: string | null;
  final_text?: string | null;
  tool_names?: string[] | null;
  terminal: boolean;
  emitted_at_utc: string;
  budget_request_id?: string | null;
  requested_rounds?: number | null;
  work_id?: string | null;
  tool_run_id?: string | null;
  tool_name?: string | null;
  tool_status?: string | null;
  tool_input_summary?: string | null;
  tool_output_summary?: string | null;
  tool_round?: number | null;
  tool_artifact_refs?: ToolArtifactRef[] | null;
  ui_artifact?: {
    artifact_id: string;
    mime: string;
    label: string;
    presentation: string;
    byte_size?: number | null;
    height_px?: number | null;
  } | null;
  previous_artifact_id?: string | null;
  root_artifact_id?: string | null;
  /** Human-facing status whisper (Home default). */
  operator_message?: string | null;
  /** Engine telemetry — shown only with engine details enabled. */
  debug_message?: string | null;
  browser_session_id?: string | null;
  browser_challenge_url?: string | null;
  /** Turn-start context budget breakdown (Cursor-style telemetry). */
  context_usage?: ContextUsageReport | null;
}

export interface ContextUsageLayer {
  id: string;
  label: string;
  chars: number;
  tokens_estimate: number;
}

export interface ContextUsageReport {
  layers: ContextUsageLayer[];
  total_tokens_estimate: number;
  total_chars: number;
  context_limit_tokens?: number | null;
  tool_count: number;
  estimator: string;
}
