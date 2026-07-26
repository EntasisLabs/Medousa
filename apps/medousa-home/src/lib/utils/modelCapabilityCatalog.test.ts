import { describe, expect, it } from "vitest";
import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
import {
  formatOutputPricingBadge,
  formatPricingBadge,
  modelHasVision,
  modelMetaLine,
} from "./modelCapabilityCatalog";

function record(overrides: Partial<ModelCapabilityRecord> = {}): ModelCapabilityRecord {
  return {
    provider: "venice",
    modelId: "aion-labs-aion-3-0",
    displayName: null,
    inputModalities: ["text"],
    outputModalities: ["text"],
    supportsVision: false,
    source: "catalog",
    fetchedAt: "2026-07-26T00:00:00Z",
    ...overrides,
  };
}

describe("formatPricingBadge", () => {
  it("renders dollars per million for paid tiers", () => {
    expect(formatPricingBadge({ promptPerTokenUsd: 4.31e-6 })).toBe("$4.31/M in");
    expect(formatOutputPricingBadge({ completionPerTokenUsd: 8.63e-6 })).toBe("$8.63/M out");
  });

  it("drops cents for expensive models", () => {
    expect(formatPricingBadge({ promptPerTokenUsd: 5.75e-5 })).toBe("$58/M in");
  });

  it("keeps sub-dollar pricing readable instead of exponential", () => {
    expect(formatPricingBadge({ promptPerTokenUsd: 1.5e-7 })).toBe("$0.15/M in");
  });

  it("returns null for missing or free pricing", () => {
    expect(formatPricingBadge(undefined)).toBeNull();
    expect(formatPricingBadge({ promptPerTokenUsd: 0 })).toBeNull();
    expect(formatOutputPricingBadge({ promptPerTokenUsd: 1e-6 })).toBeNull();
  });
});

describe("modelMetaLine", () => {
  it("joins provider, both prices, and context", () => {
    expect(
      modelMetaLine(
        record({
          maxInputTokens: 128_000,
          pricing: { promptPerTokenUsd: 4.31e-6, completionPerTokenUsd: 8.63e-6 },
        }),
        "venice",
      ),
    ).toBe("venice · $4.31/M in · $8.63/M out · 128K ctx");
  });

  it("formats million-token windows", () => {
    expect(modelMetaLine(record({ maxInputTokens: 1_000_000 }), null)).toBe("1M ctx");
  });

  it("falls back to the provider alone when the registry has no numbers", () => {
    expect(modelMetaLine(undefined, "anthropic")).toBe("anthropic");
  });

  it("returns null when there is nothing to say", () => {
    expect(modelMetaLine(undefined, null)).toBeNull();
    expect(modelMetaLine(record(), "  ")).toBeNull();
  });
});

describe("modelHasVision", () => {
  it("reads the capability map by provider/model key", () => {
    const map = new Map<string, ModelCapabilityRecord>([
      ["anthropic:claude-opus-4-8", record({ supportsVision: true })],
    ]);
    expect(modelHasVision(map, "anthropic", "claude-opus-4-8")).toBe(true);
    expect(modelHasVision(map, "anthropic", "claude-haiku-4-5")).toBe(false);
  });
});
