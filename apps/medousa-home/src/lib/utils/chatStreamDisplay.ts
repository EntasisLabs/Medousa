import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

export function isEngineTelemetryText(message: string | null | undefined): boolean {
  const trimmed = message?.trim() ?? "";
  if (!trimmed) return false;
  if (trimmed.startsWith("◈")) return true;
  if (/orchestrator=|fallback=/i.test(trimmed)) return true;
  if (/^tool=/i.test(trimmed)) return true;
  return false;
}

/** Engine/TUI telemetry — hidden from chat unless the operator enables engine details. */
export function isEngineTelemetryEvent(event: InteractiveTurnStreamEvent): boolean {
  if (event.event_type === "status" && event.phase === "orchestration") {
    return true;
  }
  return isEngineTelemetryText(event.message);
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
  const message = event.message?.trim();
  if (!message) return null;
  if (!showEngineDetails && isEngineTelemetryEvent(event)) {
    return null;
  }
  return message;
}

export function shouldMirrorStatusIntoContent(
  event: InteractiveTurnStreamEvent,
  showEngineDetails: boolean,
): boolean {
  if (event.event_type !== "turn_progress") return false;
  if (showEngineDetails) return true;
  return !isEngineTelemetryEvent(event);
}
