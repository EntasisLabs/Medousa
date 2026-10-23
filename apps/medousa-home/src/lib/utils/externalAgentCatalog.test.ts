import { describe, expect, it } from "vitest";
import { EXTERNAL_AGENTS, externalAgent, isExternalRuntime } from "./externalAgentCatalog";
import { normalizeCodingRuntimePreferences } from "$lib/types/workshopDefaults";
import { agentRuntimeLabel, isExternalAgentRuntime, isProviderConversationRuntime } from "./sessionAgentRuntime";

describe("external runtime identity", () => {
  it.each(EXTERNAL_AGENTS)("preserves the $id runtime across surfaces", agent => {
    expect(isExternalRuntime(agent.id)).toBe(true);
    expect(isExternalAgentRuntime(agent.id)).toBe(true);
    expect(agentRuntimeLabel(agent.id)).toContain(agent.label);
    expect(normalizeCodingRuntimePreferences({ preferred: agent.id, fallbacks: ["medousa"] })).toEqual({ preferred: agent.id, fallbacks: ["medousa"] });
  });
  it("keeps Muse Code distinct from the Muse conversation service", () => {
    expect(isExternalRuntime("muse")).toBe(false);
    expect(isProviderConversationRuntime("muse")).toBe(true);
    expect(isProviderConversationRuntime("muse_code")).toBe(false);
    expect(externalAgent("muse_code").label).toBe("Muse Code");
    expect(new Set(EXTERNAL_AGENTS.map(agent => agent.id)).size).toBe(8);
  });
});
