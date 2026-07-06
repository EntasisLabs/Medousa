import { describe, expect, it } from "vitest";
import {
  buildMedousaArtifactRuntimeClientScript,
  isMedousaArtifactProbeResult,
  isMedousaArtifactRuntimeEvent,
  MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID,
} from "./medousaArtifactRuntimeClient";

describe("medousaArtifactRuntimeClient", () => {
  it("injects runtime bridge script with marker id", () => {
    const html = buildMedousaArtifactRuntimeClientScript();
    expect(html).toContain(MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID);
    expect(html).toContain("medousa:artifact:runtime");
    expect(html).toContain("medousa:artifact:probe");
  });

  it("recognizes runtime postMessages", () => {
    expect(
      isMedousaArtifactRuntimeEvent({
        type: "medousa:artifact:runtime",
        level: "error",
        message: "thoughts.slice is not a function",
        ts: Date.now(),
      }),
    ).toBe(true);
    expect(isMedousaArtifactRuntimeEvent({ type: "other" })).toBe(false);
  });

  it("recognizes probe result postMessages", () => {
    expect(
      isMedousaArtifactProbeResult({
        type: "medousa:artifact:probe:result",
        probeId: "probe-1",
        storeReady: true,
        storeRoundTripOk: true,
        errors: [],
      }),
    ).toBe(true);
  });
});
