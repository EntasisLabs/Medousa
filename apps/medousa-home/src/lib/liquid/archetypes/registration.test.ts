import { describe, expect, it } from "vitest";
import "./index";
import { registerLiquidUiFactories } from "./registerUi";
import { registry, validateTree } from "$lib/liquid/core";
import {
  hasComponent,
  loadComponent,
  resolveComponent,
} from "$lib/liquid/render/componentRegistry";
import { chatMessageToScene } from "$lib/liquid/surfaces/chat/messageToScene";
import type { ChatMessage } from "$lib/types/chat";

const ALL_IDS = [
  "prose",
  "status_pill",
  "media",
  "whisper",
  "metadata",
  "button",
  "chip",
  "callout",
  "cite",
  "section",
  "chip_group",
  "card",
  "carousel",
  "action_row",
  "observability",
  "tabs",
  "steps",
  "accordion",
  "code",
  "tree",
  "stack",
  "document",
  "compare",
  "plan",
  "timeline",
  "shortlist",
  "decision",
  "brief",
  "dashboard",
  "chart",
  "report",
  "slides",
  "thinking",
  "tool_trace",
  "presentation",
  "chat_media",
];

registerLiquidUiFactories();

describe("archetype registration", () => {
  it("registers every descriptor and lazy component factory", () => {
    for (const id of ALL_IDS) {
      expect(registry.has(id), `${id} descriptor`).toBe(true);
      expect(hasComponent(id), `${id} component`).toBe(true);
    }
  });

  it("adapter output is a valid scene (all archetypes known, required props present)", () => {
    const message = {
      id: "m1",
      role: "assistant",
      content: "Here is the answer.",
      stageWhisper: "worker handoff",
      reasoning: "thinking about it",
      streaming: false,
      toolRuns: [{ runId: "r1", toolName: "web.search", status: "succeeded", round: 1 }],
      uiArtifacts: [{ artifactId: "a1", mime: "text/html", label: "Chart", presentation: "panel" }],
    } as ChatMessage;

    const scene = chatMessageToScene(message, { statusLine: null });
    expect(validateTree(scene)).toEqual([]);
  });

  it("loads and caches a renderer only when requested", async () => {
    expect(resolveComponent("prose")).toBeNull();
    const first = await loadComponent("prose");
    expect(first).not.toBeNull();
    await expect(loadComponent("prose")).resolves.toBe(first);
    expect(resolveComponent("prose")).toBe(first);
  });
});
