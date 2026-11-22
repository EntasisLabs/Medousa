import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { COLOR_THEME_IDS, DEFAULT_COLOR_THEME } from "../src/lib/theme/themeRegistry";
import { nativeThemeCatalog, syncNativeThemes } from "./native-themes";

const NATIVE_THEMES = join(__dirname, "../../medousa-native/themes/medousa-themes.json");

describe("native theme catalog", () => {
  it("carries every registered theme in both appearances", () => {
    const catalog = nativeThemeCatalog();
    expect(catalog.defaultTheme).toBe(DEFAULT_COLOR_THEME);
    expect(catalog.themes.map((theme) => theme.id).sort()).toEqual([...COLOR_THEME_IDS].sort());
    for (const theme of catalog.themes) {
      for (const variant of [theme.dark, theme.light]) {
        for (const role of ["canvas", "chrome", "pane", "text", "text-quiet", "selection", "link"]) {
          expect(variant.roles[role], `${theme.id} ${role}`).toHaveLength(3);
        }
        expect(variant.surface["950"], theme.id).toHaveLength(3);
        expect(variant.primary["600"], theme.id).toHaveLength(3);
        expect(variant.chromeAlpha).toBeGreaterThan(0);
        expect(variant.controlRadius).toBeGreaterThan(0);
      }
    }
  });

  it("matches the file the native shell embeds", () => {
    expect(
      syncNativeThemes(NATIVE_THEMES, true),
      "Run `npm run themes:native` in apps/medousa-home",
    ).toBe(true);
  });
});
