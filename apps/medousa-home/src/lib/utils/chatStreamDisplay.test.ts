import { describe, expect, it } from "vitest";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import {
  operatorStreamErrorDetail,
  operatorStreamErrorLine,
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
