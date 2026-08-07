import { describe, expect, it } from "vitest";
import {
  CODE_SYNTAX_THEME_IDS,
  DEFAULT_CODE_SYNTAX_THEME,
  buildCodeSyntaxThemeExtensions,
  cycleCodeSyntaxTheme,
  getCodeSyntaxTheme,
  listCodeSyntaxThemes,
  resolveCodeSyntaxTheme,
} from "./codeSyntaxThemes";

describe("codeSyntaxThemes", () => {
  it("defaults unknown ids to dark-plus", () => {
    expect(resolveCodeSyntaxTheme(null)).toBe(DEFAULT_CODE_SYNTAX_THEME);
    expect(resolveCodeSyntaxTheme("nope")).toBe("dark-plus");
  });

  it("lists six industry packs", () => {
    expect(listCodeSyntaxThemes().map((theme) => theme.id)).toEqual([
      ...CODE_SYNTAX_THEME_IDS,
    ]);
    expect(CODE_SYNTAX_THEME_IDS).toHaveLength(6);
    expect(listCodeSyntaxThemes()[0]?.tagline.length).toBeGreaterThan(0);
  });

  it("builds extensions for every pack", () => {
    for (const id of CODE_SYNTAX_THEME_IDS) {
      const theme = getCodeSyntaxTheme(id);
      expect(theme.label.length).toBeGreaterThan(0);
      expect(theme.tokens.keyword.startsWith("#")).toBe(true);
      expect(buildCodeSyntaxThemeExtensions(id).length).toBeGreaterThanOrEqual(2);
    }
  });

  it("cycles through themes", () => {
    expect(cycleCodeSyntaxTheme("dark-plus")).toBe("one-dark");
    expect(cycleCodeSyntaxTheme("dracula")).toBe("dark-plus");
  });

  it("marks github-light as a light pack", () => {
    expect(getCodeSyntaxTheme("github-light").dark).toBe(false);
    expect(getCodeSyntaxTheme("dark-plus").dark).toBe(true);
  });
});
