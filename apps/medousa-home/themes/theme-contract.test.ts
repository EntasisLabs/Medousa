import { describe, expect, it } from "vitest";
import { allThemes } from "./theme-catalog";
import { validateThemeConfig } from "./theme-contract";
import { SKELETON_THEME_NAMES } from "$lib/theme/themeRegistry";

function rgb(value: string): [number, number, number] {
  const channels = value.split(/\s+/).map(Number);
  if (channels.length !== 3 || channels.some((channel) => !Number.isFinite(channel))) {
    throw new Error(`Expected an RGB triplet, received ${value}`);
  }
  return channels as [number, number, number];
}

function luminance(value: string): number {
  const channels = rgb(value).map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
}

function contrast(a: string, b: string): number {
  const lighter = Math.max(luminance(a), luminance(b));
  const darker = Math.min(luminance(a), luminance(b));
  return (lighter + 0.05) / (darker + 0.05);
}

describe("theme contract", () => {
  it("gives every shipped theme a complete, unique configuration", () => {
    const names = allThemes.map((theme) => theme.name);
    expect(new Set(names).size).toBe(names.length);
    const registeredNames = Object.values(SKELETON_THEME_NAMES).flatMap((pair) => [
      pair.dark,
      pair.light,
    ]);
    expect(new Set(names)).toEqual(new Set(registeredNames));
    for (const theme of allThemes) {
      expect(validateThemeConfig(theme), theme.name).toEqual([]);
    }
  });

  it("keeps body text and primary actions readable", () => {
    for (const theme of allThemes) {
      const properties = theme.properties as Record<string, string>;
      expect(
        contrast(properties["--theme-text"]!, properties["--theme-canvas"]!),
        `${theme.name} body contrast`,
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        contrast(properties["--on-primary"]!, properties["--theme-action"]!),
        `${theme.name} primary action contrast`,
      ).toBeGreaterThanOrEqual(4.5);
      for (const status of ["error", "success", "warning"] as const) {
        expect(
          contrast(
            properties[`--on-${status}`]!,
            properties[`--color-${status}-500`]!,
          ),
          `${theme.name} ${status} contrast`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("gives every Medousa colorway a distinct foundation", () => {
    const markDarkThemes = allThemes.filter((theme) =>
      theme.name.startsWith("mark-") && theme.name.endsWith("-dark"),
    );
    const canvases = markDarkThemes.map(
      (theme) => (theme.properties as Record<string, string>)["--theme-canvas"],
    );
    expect(markDarkThemes).toHaveLength(10);
    expect(new Set(canvases).size).toBe(markDarkThemes.length);
  });
});
