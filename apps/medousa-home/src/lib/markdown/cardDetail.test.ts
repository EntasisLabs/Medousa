import { describe, expect, it } from "vitest";
import { cardHasDetail } from "$lib/markdown/liquidEmbeds";

describe("cardHasDetail", () => {
  it("is false for flat strip cards", () => {
    expect(cardHasDetail({})).toBe(false);
    expect(cardHasDetail({ meta: "  ", summary: "", chips: [], points: [] })).toBe(false);
  });

  it("is true when any structured detail field is present", () => {
    expect(cardHasDetail({ meta: "Caveat" })).toBe(true);
    expect(cardHasDetail({ summary: "Long form" })).toBe(true);
    expect(cardHasDetail({ chips: ["Benchmarks"] })).toBe(true);
    expect(cardHasDetail({ points: [{ label: "A", body: "B" }] })).toBe(true);
  });
});
