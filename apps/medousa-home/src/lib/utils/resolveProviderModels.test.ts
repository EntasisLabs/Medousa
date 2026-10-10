import { describe, expect, it, vi } from "vitest";
import type { ProviderCatalogEntry } from "$lib/types/providers";
import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
import { filterRecordsForCapability, pickModelFromRecords, resolveModelsForProvider } from "./resolveProviderModels";

const accountModels = vi.hoisted(() => vi.fn());
vi.mock("./chatgptOAuth", () => ({ listChatGptOAuthModels: accountModels }));
vi.mock("./providerSettings", () => ({
  resolveRuntimeProviderId: async (id: string) => id,
  resolveProviderBaseUrl: async () => null,
}));

const accountProvider: ProviderCatalogEntry = {
  id: "openai-codex", label: "ChatGPT account", category: "cloud",
  defaultModel: "unavailable-default", needsApiKey: false,
  supportsCustomBaseUrl: false, defaultBaseUrl: null, keyHint: null, blurb: "",
};

describe("ChatGPT account catalog", () => {
  it("preserves account ordering and display names", async () => {
    accountModels.mockResolvedValueOnce({
      models: ["gpt-6-luna", "gpt-6.1-sol"],
      display_names: { "gpt-6-luna": "Account Luna", "gpt-6.1-sol": "Account Sol" },
    });
    const records = await resolveModelsForProvider(accountProvider);
    expect(records.map((record) => record.modelId)).toEqual(["gpt-6-luna", "gpt-6.1-sol"]);
    expect(records.map((record) => record.displayName)).toEqual(["Account Luna", "Account Sol"]);
  });

  it("does not restore cached or default models after discovery fails", async () => {
    accountModels.mockRejectedValueOnce(new Error("account changed"));
    expect(await resolveModelsForProvider(accountProvider)).toEqual([]);
  });

  it("keeps an empty account catalog empty", async () => {
    accountModels.mockResolvedValueOnce({ models: [], display_names: {} });
    expect(await resolveModelsForProvider(accountProvider)).toEqual([]);
  });
});

function rec(modelId: string): ModelCapabilityRecord {
  return {
    provider: "ollama",
    modelId,
    displayName: modelId,
    inputModalities: ["text"],
    outputModalities: ["text"],
    supportsVision: false,
    source: "live",
    fetchedAt: new Date().toISOString(),
  };
}

describe("pickModelFromRecords", () => {
  it("prefers suggested when present in the list", () => {
    expect(
      pickModelFromRecords([rec("mistral"), rec("llama3.1")], {
        preferred: "llama3.1",
        fallbackDefault: "llama3.2",
      }),
    ).toBe("llama3.1");
  });

  it("keeps current when still valid", () => {
    expect(
      pickModelFromRecords([rec("a"), rec("b")], {
        current: "b",
        fallbackDefault: "a",
      }),
    ).toBe("b");
  });

  it("falls back to first record then default", () => {
    expect(
      pickModelFromRecords([rec("first"), rec("second")], {
        preferred: "missing",
        fallbackDefault: "llama3.2",
      }),
    ).toBe("first");
    expect(
      pickModelFromRecords([], { fallbackDefault: "llama3.2" }),
    ).toBe("llama3.2");
  });
});

describe("filterRecordsForCapability", () => {
  it("only offers image-capable models for an image role", () => {
    const text = rec("text-only");
    const vision = { ...rec("vision"), supportsVision: true };
    expect(filterRecordsForCapability([text, vision], "vision")).toEqual([vision]);
    expect(filterRecordsForCapability([text, vision], "text")).toEqual([text, vision]);
  });
});
