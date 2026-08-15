import type { TurnStreamEnvelopeV2 } from "$lib/types/generated/daemon_api";
import {
  isTerminalTurnStreamEventV2,
  terminalTurnStreamTextV2,
} from "$lib/stream/v2";

export type CompanionFeedbackTone = "success" | "error" | "attention";

export interface CompanionFeedback {
  tone: CompanionFeedbackTone;
  message: string;
}

export interface CompanionActivity {
  activeTurnIds: Set<string>;
  feedback: CompanionFeedback | null;
}

export interface CompanionEventResult extends CompanionActivity {
  approvalChanged: boolean;
}

export function initialCompanionActivity(): CompanionActivity {
  return {
    activeTurnIds: new Set(),
    feedback: null,
  };
}

function concise(value: string | null | undefined, fallback: string): string {
  const normalized = value?.replace(/\s+/g, " ").trim();
  if (!normalized) return fallback;
  return normalized.length > 132 ? `${normalized.slice(0, 131)}…` : normalized;
}

export function applyCompanionStreamEvent(
  activity: CompanionActivity,
  envelope: TurnStreamEnvelopeV2,
): CompanionEventResult {
  const activeTurnIds = new Set(activity.activeTurnIds);
  let feedback = activity.feedback;
  let approvalChanged = false;
  const event = envelope.event;

  if (event.type === "budget_approval_required") {
    activeTurnIds.delete(envelope.turn_id);
    feedback = {
      tone: "attention",
      message: concise(
        event.progress_summary || event.reason,
        "Medousa needs approval to continue.",
      ),
    };
    approvalChanged = true;
  } else if (event.type === "permission_request") {
    activeTurnIds.delete(envelope.turn_id);
    feedback = {
      tone: "attention",
      message: concise(event.message, "An agent needs permission to continue."),
    };
  } else if (event.type === "error") {
    activeTurnIds.delete(envelope.turn_id);
    feedback = {
      tone: "error",
      message: concise(event.operator_message, "That turn could not finish."),
    };
  } else if (isTerminalTurnStreamEventV2(event)) {
    activeTurnIds.delete(envelope.turn_id);
    feedback = {
      tone: "success",
      message: concise(terminalTurnStreamTextV2(event), "Done."),
    };
  } else {
    activeTurnIds.add(envelope.turn_id);
  }

  return { activeTurnIds, feedback, approvalChanged };
}

export function companionSpriteState(input: {
  connected: boolean;
  expanded: boolean;
  sending: boolean;
  activeTurnCount: number;
  pendingApproval: boolean;
  feedbackTone?: CompanionFeedbackTone | null;
}): "attention" | "error" | "float" | "launch" | "loading" | "success" {
  if (!input.connected || input.feedbackTone === "error") return "error";
  if (input.pendingApproval || input.feedbackTone === "attention") return "attention";
  if (input.sending) return "launch";
  if (input.activeTurnCount > 0) return "loading";
  if (input.feedbackTone === "success") return "success";
  if (input.expanded) return "attention";
  return "float";
}
