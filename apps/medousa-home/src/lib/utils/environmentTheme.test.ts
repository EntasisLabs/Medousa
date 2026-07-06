import { describe, expect, it } from "vitest";
import { resolveEnvironmentTheme } from "./environmentTheme";

describe("resolveEnvironmentTheme", () => {
  it("prefers environment theme over workshop fallback", () => {
    const resolved = resolveEnvironmentTheme(
      {
        version: 1,
        profileId: "default",
        surfaces: [],
        components: [],
        updatedAt: "",
        updatedBy: "test",
        theme: { colorThemeId: "dracula", brandColor: "#bd93f9", tagline: "Goth desk" },
      },
      "medousa",
      "#ff0000",
      true,
    );
    expect(resolved.colorThemeId).toBe("dracula");
    expect(resolved.brandColor).toBe("#bd93f9");
    expect(resolved.tagline).toBe("Goth desk");
    expect(resolved.tokens.brand).toBe("#bd93f9");
    expect(resolved.tokens.accent).toBeTruthy();
  });

  it("falls back to workshop theme when spec.theme is empty", () => {
    const resolved = resolveEnvironmentTheme(
      null,
      "tokyo-night",
      "#7aa2f7",
      true,
    );
    expect(resolved.colorThemeId).toBe("tokyo-night");
    expect(resolved.brandColor).toBe("#7aa2f7");
  });
});
