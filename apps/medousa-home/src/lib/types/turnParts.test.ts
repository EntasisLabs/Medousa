import { describe, expect, it } from "vitest";

import {
  composeTurnMarkdown,
  progressFromParts,
  toolRunsFromParts,
  uiArtifactsFromParts,
  type TurnPart,
} from "$lib/types/turnParts";

describe("turnParts", () => {
  it("parses progress parts from persisted timeline JSON", () => {
    const parts = JSON.parse(
      '[{"kind":"tool_run","run_id":"tr-1","tool_name":"cognition_memory_context","status":"succeeded","input_summary":"session","started_at":"2026-06-25T12:00:00Z"},{"kind":"progress","markdown":"Pulling context…"},{"kind":"text","markdown":"Final answer."}]',
    ) as TurnPart[];

    expect(toolRunsFromParts(parts)?.[0]?.toolName).toBe("cognition_memory_context");
    expect(progressFromParts(parts)).toBe("Pulling context…");
    expect(composeTurnMarkdown("Final answer.", parts)).toContain("> [!note] Progress");
  });

  it("uses the latest progress note when several exist", () => {
    const parts: TurnPart[] = [
      { kind: "progress", markdown: "Step one" },
      { kind: "progress", markdown: "Step two" },
      { kind: "text", markdown: "Done." },
    ];
    expect(progressFromParts(parts)).toBe("Step two");
  });

  it("maps attachment_ref parts to ui artifacts", () => {
    const parts: TurnPart[] = [
      {
        kind: "attachment_ref",
        artifact_id: "art:demo:ui:abc",
        mime: "text/html",
        label: "Chart",
        byte_size: 1200,
        presentation: "panel",
        height_px: 480,
      },
      { kind: "text", markdown: "See panel." },
    ];

    expect(uiArtifactsFromParts(parts)).toEqual([
      {
        artifactId: "art:demo:ui:abc",
        mime: "text/html",
        label: "Chart",
        presentation: "panel",
        byteSize: 1200,
        heightPx: 480,
      },
    ]);
  });
});
