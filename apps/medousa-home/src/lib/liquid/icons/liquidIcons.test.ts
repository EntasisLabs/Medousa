import { describe, expect, it } from "vitest";
import {
  looksLikeLiquidIconId,
  normalizeLiquidIconId,
  resolveLiquidGlyph,
} from "./liquidIcons";

describe("liquidIcons", () => {
  it("normalizes allowlisted kebab and camel ids", () => {
    expect(normalizeLiquidIconId("sparkles")).toBe("sparkles");
    expect(normalizeLiquidIconId("MessageCircle")).toBe("message-circle");
    expect(normalizeLiquidIconId("map_pin")).toBe("map-pin");
    expect(normalizeLiquidIconId("plane")).toBe("plane");
    expect(normalizeLiquidIconId("not-real")).toBeNull();
  });

  it("detects icon ids vs emoji", () => {
    expect(looksLikeLiquidIconId("plane")).toBe(true);
    expect(looksLikeLiquidIconId("✈️")).toBe(false);
    expect(looksLikeLiquidIconId("two words")).toBe(false);
  });

  it("prefers icon over emoji and resolves emoji-as-id", () => {
    const viaIcon = resolveLiquidGlyph({ icon: "plane", emoji: "✈️" });
    expect(viaIcon?.kind).toBe("icon");
    if (viaIcon?.kind === "icon") expect(viaIcon.id).toBe("plane");

    const viaEmojiId = resolveLiquidGlyph({ emoji: "rocket" });
    expect(viaEmojiId?.kind).toBe("icon");

    const viaEmoji = resolveLiquidGlyph({ emoji: "✨" });
    expect(viaEmoji).toEqual({ kind: "text", text: "✨" });
  });
});
