export interface ToolArtifactRef {
  role: string;
  content_type: string;
  byte_size: number;
  hash64: string;
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
  /** Turn index in session history (1-based, matches slice_id turn:N). */
  turnIndex?: number | null;

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
  /** Human-facing status whisper (Home default). */
  operator_message?: string | null;
  /** Engine telemetry — shown only with engine details enabled. */
  debug_message?: string | null;
}
