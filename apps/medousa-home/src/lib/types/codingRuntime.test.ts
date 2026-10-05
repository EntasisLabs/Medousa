import { describe, expect, it } from "vitest";
import { normalizeCodingRuntimePreferences, normalizeWorkshopDefaults } from "./workshopDefaults";

describe("coding runtime preferences", () => {
  it("upgrades old settings to Medousa without external fallbacks", () => {
    expect(normalizeWorkshopDefaults({ provider: "openai", model: "model" }).codingRuntime)
      .toEqual({ preferred: "medousa", fallbacks: [] });
  });
  it("preserves preference and fallback order through settings round trips", () => {
    const settings = normalizeWorkshopDefaults({ codingRuntime: { preferred: "cursor", fallbacks: ["hermes", "codex", "medousa"] } });
    expect(normalizeWorkshopDefaults(JSON.parse(JSON.stringify(settings))).codingRuntime)
      .toEqual({ preferred: "cursor", fallbacks: ["hermes", "codex", "medousa"] });
  });
  it("removes repeated preferred and fallback entries", () => {
    expect(normalizeCodingRuntimePreferences({ preferred: "codex", fallbacks: ["codex", "cursor", "cursor", "medousa"] }))
      .toEqual({ preferred: "codex", fallbacks: ["cursor", "medousa"] });
  });
});
