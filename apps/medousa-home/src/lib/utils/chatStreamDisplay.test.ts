import { describe, expect, it } from "vitest";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import {
  operatorStreamErrorDetail,
  operatorStreamErrorLine,
  operatorStreamStatusLine,
  visibleChatStatusLine,
} from "./chatStreamDisplay";

function errorEvent(
  partial: Partial<InteractiveTurnStreamEvent>,
): InteractiveTurnStreamEvent {
  return {
    event_type: "error",
    turn_id: "t1",
    phase: "failed",
    terminal: true,
    ...partial,
  } as InteractiveTurnStreamEvent;
}

describe("operatorStreamErrorDetail", () => {
  it("returns debug when distinct from the friendly operator line", () => {
    const event = errorEvent({
      operator_message: "The model could not complete this turn.",
      debug_message: "ollama: model 'llama3.2' not found (404)",
    });
    const friendly = operatorStreamErrorLine(event, false);
    expect(friendly).toBe("The model could not complete this turn.");
    expect(operatorStreamErrorDetail(event, friendly)).toBe(
      "ollama: model 'llama3.2' not found (404)",
    );
  });

  it("returns null when debug matches the friendly line", () => {
    const event = errorEvent({
      operator_message: "same text",
      debug_message: "same text",
    });
    const friendly = operatorStreamErrorLine(event, false);
    expect(operatorStreamErrorDetail(event, friendly)).toBeNull();
  });

  it("returns null when there is no debug payload", () => {
    const event = errorEvent({
      operator_message: "Something went wrong.",
    });
    const friendly = operatorStreamErrorLine(event, false);
    expect(operatorStreamErrorDetail(event, friendly)).toBeNull();
  });
});

describe("startup status noise", () => {
  it.each([false, true])("hides native and external acknowledgements (engine details: %s)", (details) => {
    for (const agent_runtime of [undefined, "codex"]) {
      expect(operatorStreamStatusLine({
        event_type: "status", turn_id: "t1", phase: "accepted",
        operator_message: "Interactive turn accepted; agent runtime started",
        agent_runtime,
      } as InteractiveTurnStreamEvent, details)).toBeNull();
    }
    for (const text of [
      "Interactive turn accepted; agent runtime started",
      "ingest accepted; agent runtime started",
      "Agent runtime started", "Agent runtime accepted",
    ]) {
      expect(visibleChatStatusLine(text, details)).toBeNull();
      expect(operatorStreamStatusLine({
        event_type: "status", turn_id: "t1", message: text,
      } as InteractiveTurnStreamEvent, details)).toBeNull();
    }
  });

  it("keeps actionable progress and errors visible", () => {
    const message = "Waiting for permission to run the build";
    expect(operatorStreamStatusLine({
      event_type: "status", turn_id: "t1", phase: "blocked",
      operator_message: message,
    } as InteractiveTurnStreamEvent, false)).toBe(message);
    expect(visibleChatStatusLine(message, false)).toBe(message);
    expect(operatorStreamErrorLine(errorEvent({
      operator_message: "Agent runtime failed to start",
    }), false)).toBe("Agent runtime failed to start");
  });
});
