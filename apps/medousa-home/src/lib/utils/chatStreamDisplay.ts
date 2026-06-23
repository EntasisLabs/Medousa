import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

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
  if (showEngineDetails) {
    const debug = event.debug_message?.trim();
    if (debug) return debug;
  }
  const legacy = event.message?.trim();
  if (legacy && !isEngineTelemetryText(legacy)) return legacy;
  return "Something went wrong on this turn. Try again in a moment.";
}

export function shouldMirrorStatusIntoContent(
  event: InteractiveTurnStreamEvent,
  showEngineDetails: boolean,
): boolean {
  if (event.event_type !== "turn_progress") return false;
  if (showEngineDetails) return true;
  return streamOperatorMessage(event) != null;
}
