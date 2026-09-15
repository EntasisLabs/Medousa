import { describe, expect, it } from "vitest";
import { livePhaseForServerEvent } from "$lib/liveVoice";

describe("Medousa Live server events", () => {
  it("maps speech and response events to user-visible phases", () => {
    expect(livePhaseForServerEvent("input_audio_buffer.speech_started")).toBe("listening");
    expect(livePhaseForServerEvent("response.started")).toBe("thinking");
    expect(livePhaseForServerEvent("response.output_audio.started")).toBe("speaking");
    expect(livePhaseForServerEvent("response.done")).toBe("listening");
  });

  it("ignores unrelated events", () => {
    expect(livePhaseForServerEvent("session.started")).toBeNull();
  });
});
