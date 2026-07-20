import { describe, expect, it } from "vitest";
import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
import { pickModelFromRecords } from "./resolveProviderModels";

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
