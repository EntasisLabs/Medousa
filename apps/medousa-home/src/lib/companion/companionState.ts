import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import {
  isBudgetApprovalStreamEvent,
  isPermissionRequestStreamEvent,
} from "$lib/utils/streamEvents";

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
  event: InteractiveTurnStreamEvent,
): CompanionEventResult {
  const activeTurnIds = new Set(activity.activeTurnIds);
  let feedback = activity.feedback;
  let approvalChanged = false;

  if (isBudgetApprovalStreamEvent(event)) {
    activeTurnIds.delete(event.turn_id);
    feedback = {
      tone: "attention",
      message: concise(
        event.operator_message || event.message,
        "Medousa needs approval to continue.",
      ),
    };
    approvalChanged = true;
  } else if (isPermissionRequestStreamEvent(event)) {
    activeTurnIds.delete(event.turn_id);
    feedback = {
      tone: "attention",
      message: concise(
        event.operator_message || event.message,
        "An agent needs permission to continue.",
      ),
    };
  } else if (event.event_type === "error") {
    activeTurnIds.delete(event.turn_id);
    feedback = {
      tone: "error",
      message: concise(
        event.operator_message || event.message,
        "That turn could not finish.",
      ),
    };
  } else if (event.terminal) {
    activeTurnIds.delete(event.turn_id);
    feedback = {
      tone: "success",
      message: concise(
        event.operator_message || event.final_text || event.message,
        "Done.",
      ),
    };
  } else {
    activeTurnIds.add(event.turn_id);
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
