import { describe, expect, it } from "vitest";
import { mapBrowserTurnEvent } from "./browserDaemon";

describe("browser workshop turn events", () => {
  it("maps a model delta into a non-terminal stream event", () => {
    const event = mapBrowserTurnEvent({
      kind: "delta",
      turn_id: "turn-1",
      text: "Hello",
    });
    expect(event.turn_id).toBe("turn-1");
    expect(event.terminal).toBe(false);
    expect(event.content_delta).toBe("Hello");
  });

  it("maps completion into a terminal final event", () => {
    const event = mapBrowserTurnEvent({
      kind: "done",
      turn_id: "turn-1",
      text: "Hello there",
    });
    expect(event.terminal).toBe(true);
    expect(event.event_type).toBe("final");
    expect(event.final_text).toBe("Hello there");
  });
});
