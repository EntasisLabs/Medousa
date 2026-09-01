import { describe, expect, it } from "vitest";

import {
  chatSegmentsFromParts,
  composeTurnMarkdown,
  hostContextFromParts,
  hostContextLabel,
  modelReceiptFromParts,
  progressFromParts,
  toolRunsFromParts,
  uiArtifactsFromParts,
  type TurnPart,
} from "$lib/types/turnParts";

describe("turnParts", () => {
  it("returns the daemon-observed inference route", () => {
    expect(
      modelReceiptFromParts([
        { kind: "model_receipt", provider: "openai-codex", model: "gpt-5.6-sol" },
      ]),
    ).toEqual({ provider: "openai-codex", model: "gpt-5.6-sol" });
  });

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

  it("hydrates native V3 parts as an honest chronological segment timeline", () => {
    const parts: TurnPart[] = [
      {
        kind: "text",
        markdown: "Let me check.",
        segment_id: "segment-a",
        model_round: 1,
      },
      {
        kind: "tool_run",
        run_id: "run-1",
        tool_name: "vault.read",
        status: "succeeded",
        input_summary: "read note",
        input_params: [{ key: "path", value: "notes/plan.md", truncated: false }],
        tool_round: 1,
      },
      {
        kind: "tool_run",
        run_id: "run-2",
        tool_name: "web.search",
        status: "succeeded",
        input_summary: "search topic",
        tool_round: 2,
      },
      {
        kind: "progress",
        markdown: "I found the relevant entries.",
      },
      {
        kind: "text",
        markdown: "Found it.",
        segment_id: "segment-b",
        model_round: 2,
      },
    ];

    const segments = chatSegmentsFromParts(parts);
    expect(segments?.map((segment) => segment.kind)).toEqual([
      "text",
      "tool_group",
      "progress",
      "text",
    ]);
    expect(segments?.[1]).toEqual(
      expect.objectContaining({
        kind: "tool_group",
        runs: [
          expect.objectContaining({
            runId: "run-1",
            inputParams: [{ key: "path", value: "notes/plan.md", truncated: false }],
          }),
          expect.objectContaining({ runId: "run-2" }),
        ],
      }),
    );
    expect(segments?.[2]).toEqual(
      expect.objectContaining({
        kind: "progress",
        markdown: "I found the relevant entries.",
      }),
    );
  });

  it("leaves legacy parts on the legacy layout instead of inventing chronology", () => {
    expect(
      chatSegmentsFromParts([
        { kind: "text", markdown: "Old answer" },
        {
          kind: "tool_run",
          run_id: "run-1",
          tool_name: "vault.read",
          status: "succeeded",
          input_summary: "read note",
        },
      ]),
    ).toBeUndefined();
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

  it("projects host context into a concise label without journal text", () => {
    const parts: TurnPart[] = [
      { kind: "text", markdown: "Explain this" },
      {
        kind: "host_context",
        context: {
          source: "vscode",
          resource_kind: "file",
          resource_path: "/work/src/main.ts",
          selection: {
            text: "const answer = 42;",
            start: { line: 9, character: 0 },
            end: { line: 11, character: 1 },
          },
          diagnostics: [{ message: "Example warning" }],
          related_resources: [],
        },
      },
    ];

    const context = hostContextFromParts(parts);
    expect(hostContextLabel(context)).toBe("VS Code · main.ts · lines 10–12 · 1 diagnostic");
    expect(composeTurnMarkdown("Explain this", parts)).toBe("Explain this");
  });
});
