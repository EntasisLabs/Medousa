import { describe, expect, it } from "vitest";
import {
  COLOR_THEME_OPTIONS,
  SKELETON_THEME_NAMES,
} from "$lib/theme/themeRegistry";
import { MEDOUSA_MARK_OPTIONS } from "$lib/theme/medousaMarks";

describe("logo-derived themes", () => {
  it("keeps one paired color theme for every approved mark", () => {
    for (const mark of MEDOUSA_MARK_OPTIONS) {
      const theme = COLOR_THEME_OPTIONS.find((option) => option.id === mark.pairedThemeId);
      expect(theme?.group).toBe("medousa-marks");
      expect(SKELETON_THEME_NAMES[mark.pairedThemeId]).toBeTruthy();
    }
  });

  it("adds all ten mark palettes without replacing existing themes", () => {
    expect(COLOR_THEME_OPTIONS.filter((option) => option.group === "medousa-marks")).toHaveLength(10);
    expect(COLOR_THEME_OPTIONS.some((option) => option.id === "medousa")).toBe(true);
    expect(COLOR_THEME_OPTIONS.some((option) => option.id === "black-lily")).toBe(true);
  });

  it("gives every mark a theme-aware companion preview", () => {
    for (const mark of MEDOUSA_MARK_OPTIONS) {
      expect(mark.lightPreviewBackground).not.toBe("#000000");
      expect(mark.darkPreviewBackground).toBeTruthy();
      expect(mark.lightPreviewForeground).toBeTruthy();
      expect(mark.darkPreviewForeground).toBeTruthy();
    }
  });
});
