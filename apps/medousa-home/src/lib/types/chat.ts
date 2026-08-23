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
  /** Stable lineage id for MedousaStore scope across revisions. */
  rootArtifactId?: string | null;
}

export interface ToolRunState {
  runId: string;
  toolName: string;
  status: "running" | "succeeded" | "failed";
  round: number;
  inputSummary?: string | null;
  /** Redacted arguments, so evidence reads `query: "…"` and not just the tool name. */
  inputParams?: import("$lib/types/card").ToolInputParam[];
  outputSummary?: string | null;
  artifactRefs?: ToolArtifactRef[];
}

import type { ChatMediaAttachment } from "$lib/types/media";
import type {
  ContextUsageLayer as GeneratedContextUsageLayer,
  ContextUsageReport as GeneratedContextUsageReport,
  HostTurnContext,
  InteractiveTurnStreamEvent as GeneratedInteractiveTurnStreamEvent,
  StreamUiScene as GeneratedStreamUiScene,
} from "$lib/types/generated/daemon_api";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  streaming?: boolean;
  /** Media attachments rendered with the turn (images, files). */
  mediaAttachments?: ChatMediaAttachment[];
  /** Structured editor, note, or page context attached to this user turn. */
  hostContext?: HostTurnContext | null;
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
  /** Daemon-observed successful inference route after fallback resolution. */
  responseProvider?: string | null;
  responseModel?: string | null;
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
  /** Raw debug/engine detail for collapsed “View details” (when distinct from errorLine). */
  errorDetail?: string | null;
  /** Turn index in session history (1-based, matches slice_id turn:N). */
  turnIndex?: number | null;
  /** Shared-room speaker profile id (`user:alice`) for human turns. */
  speakerProfileId?: string | null;
  /** Durable coordinate for a committed transcript entry. Absent for optimistic/streaming UI. */
  transcript?: {
    authorityId: string;
    sessionId: string;
    entryId: string;
    entrySeq: number;
    /** Original occurrence when this entry was materialized into a derived session. */
    source?: {
      authorityId: string;
      sessionId: string;
      entryId: string;
      entrySeq: number;
    } | null;
  } | null;
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

export interface PendingAgentPermission {
  turnId: string;
  messageId: string | null;
  /** Daemon ACP permission request id (approve/deny API). */
  requestId: string;
  agentSessionId: string | null;
  agentRuntime: string | null;
  message: string;
}

export interface PendingAgentSecret {
  turnId: string;
  messageId: string | null;
  /** Metadata-only request id; the credential value never enters chat state. */
  requestId: string;
  label: string;
  reason: string;
  providerType: string;
  credentialKey: string;
  backend: "openshell_provider" | "grapheme_runtime";
  allowedHosts: string[];
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

/** Frozen v1 compatibility DTO. The schema generator is its sole Home owner. */
export type InteractiveTurnStreamEvent = GeneratedInteractiveTurnStreamEvent;
export type StreamUiScene = GeneratedStreamUiScene;
export type ContextUsageLayer = GeneratedContextUsageLayer;
export type ContextUsageReport = GeneratedContextUsageReport;
