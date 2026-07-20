import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import { friendlyTurnError } from "$lib/utils/normieErrors";

export function isEngineTelemetryText(message: string | null | undefined): boolean {
  const trimmed = message?.trim() ?? "";
  if (!trimmed) return false;
  if (trimmed.startsWith("◈")) return true;
  if (/orchestrator=|fallback=/i.test(trimmed)) return true;
  if (/^tool=/i.test(trimmed)) return true;
  return false;
}

function streamDebugMessage(event: InteractiveTurnStreamEvent): string | null {
  const explicit = event.debug_message?.trim();
  if (explicit) return explicit;
  if (event.operator_message?.trim()) return null;
  const legacy = event.message?.trim();
  if (!legacy) return null;
  return isEngineTelemetryText(legacy) ? legacy : null;
}

function streamOperatorMessage(event: InteractiveTurnStreamEvent): string | null {
  const explicit = event.operator_message?.trim();
  if (explicit) return explicit;
  if (event.debug_message?.trim()) return null;
  const legacy = event.message?.trim();
  if (!legacy || isEngineTelemetryText(legacy)) return null;
  return legacy;
}

/** Engine/TUI telemetry — hidden from chat unless the operator enables engine details. */
export function isEngineTelemetryEvent(event: InteractiveTurnStreamEvent): boolean {
  if (event.event_type === "status" && event.phase === "orchestration") {
    return streamOperatorMessage(event) == null;
  }
  return streamDebugMessage(event) != null && streamOperatorMessage(event) == null;
}

export function visibleChatStatusLine(
  line: string | null | undefined,
  showEngineDetails: boolean,
): string | null {
  const trimmed = line?.trim();
  if (!trimmed) return null;
  if (!showEngineDetails && isEngineTelemetryText(trimmed)) return null;
  return trimmed;
}

export function operatorStreamStatusLine(
  event: InteractiveTurnStreamEvent,
  showEngineDetails: boolean,
): string | null {
  const operator = streamOperatorMessage(event);
  if (operator) return operator;
  if (showEngineDetails) {
    return streamDebugMessage(event);
  }
  return null;
}

export function operatorStreamErrorLine(
  event: InteractiveTurnStreamEvent,
  showEngineDetails: boolean,
): string {
  const operator = event.operator_message?.trim();
  if (operator) return operator;

  const debug = event.debug_message?.trim();
  const legacy = event.message?.trim();
  const raw =
    debug ?? (legacy && !isEngineTelemetryText(legacy) ? legacy : "");

  if (raw) {
    if (showEngineDetails) return raw;
    return friendlyTurnError(raw);
  }

  return "Something went wrong on this turn. Try again in a moment.";
}

/**
 * Raw debug text for a collapsed “View details” expand.
 * Returns null when identical to the friendly line (nothing extra to show).
 */
export function operatorStreamErrorDetail(
  event: InteractiveTurnStreamEvent,
  friendlyLine: string,
): string | null {
  const debug =
    event.debug_message?.trim() ||
    (event.message?.trim() && isEngineTelemetryText(event.message)
      ? event.message.trim()
      : null) ||
    event.message?.trim() ||
    null;
  if (!debug) return null;
  if (debug === friendlyLine.trim()) return null;
  return debug;
}

export function shouldMirrorStatusIntoContent(
  _event: InteractiveTurnStreamEvent,
  _showEngineDetails: boolean,
): boolean {
  // Progress whispers belong in statusLine + tool chips, not the answer body.
  return false;
}

export function shouldSuppressStreamContentDelta(message: {
  streaming?: boolean;
  toolRuns?: unknown[] | null;
}): boolean {
  if (!message.streaming) return false;
  // After the first tool receipt, interim streamed prose belongs in chips/status only.
  return (message.toolRuns?.length ?? 0) > 0;
}
