import { describe, expect, it } from "vitest";
import {
  applyMainInferenceSelection,
  syncFlatFieldsFromProfiles,
} from "$lib/types/inferenceProfiles";

describe("applyMainInferenceSelection", () => {
  it("persists provider and model in the authoritative main profile", () => {
    const selected = applyMainInferenceSelection(
      {
        provider: "anthropic",
        model: "claude-sonnet",
        baseUrl: "https://old-provider.example/v1",
        inferenceProfiles: {
          main: {
            provider: "anthropic",
            model: "claude-sonnet",
            baseUrl: "https://old-provider.example/v1",
            fallbacks: [{ provider: "openai", model: "gpt-5" }],
          },
          vision: { provider: "openai", model: "gpt-5-vision" },
        },
      },
      "openai-codex",
      "gpt-5.6-luna",
    );

    expect(selected.provider).toBe("openai-codex");
    expect(selected.model).toBe("gpt-5.6-luna");
    expect(selected.baseUrl).toBeNull();
    expect(selected.inferenceProfiles?.main).toEqual({
      provider: "openai-codex",
      model: "gpt-5.6-luna",
      baseUrl: null,
      fallbacks: [{ provider: "openai", model: "gpt-5" }],
    });
    expect(selected.inferenceProfiles?.vision?.model).toBe("gpt-5-vision");
    expect(syncFlatFieldsFromProfiles(selected)).toMatchObject({
      provider: "openai-codex",
      model: "gpt-5.6-luna",
    });
  });

  it("retains a custom endpoint when selecting another model from the same provider", () => {
    const selected = applyMainInferenceSelection(
      {
        provider: "openai",
        model: "old-model",
        baseUrl: "https://gateway.example/v1",
        inferenceProfiles: {
          main: {
            provider: "openai",
            model: "old-model",
            baseUrl: "https://gateway.example/v1",
          },
        },
      },
      "openai",
      "new-model",
    );

    expect(selected.baseUrl).toBe("https://gateway.example/v1");
    expect(selected.inferenceProfiles?.main?.baseUrl).toBe(
      "https://gateway.example/v1",
    );
  });
});
