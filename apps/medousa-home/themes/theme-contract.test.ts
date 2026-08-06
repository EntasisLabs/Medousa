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

  it("keeps body text readable on every shell surface", () => {
    const backgrounds = [
      "--theme-canvas",
      "--theme-chrome",
      "--theme-header",
      "--theme-pane",
      "--theme-pane-muted",
      "--theme-card",
      "--theme-card-hover",
    ] as const;

    for (const theme of allThemes) {
      const properties = theme.properties as Record<string, string>;
      for (const foreground of ["--theme-text", "--theme-text-secondary"] as const) {
        for (const background of backgrounds) {
          expect(
            contrast(properties[foreground]!, properties[background]!),
            `${theme.name} ${foreground} on ${background}`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
  });

  it("keeps status, link and focus roles legible against the canvas", () => {
    for (const theme of allThemes) {
      const properties = theme.properties as Record<string, string>;
      const canvas = properties["--theme-canvas"]!;

      for (const role of ["--theme-link", "--theme-error", "--theme-success", "--theme-warning"] as const) {
        expect(
          contrast(properties[role]!, canvas),
          `${theme.name} ${role} on canvas`,
        ).toBeGreaterThanOrEqual(4.5);
      }
      for (const role of ["--theme-text-disabled", "--theme-focus"] as const) {
        expect(
          contrast(properties[role]!, canvas),
          `${theme.name} ${role} on canvas`,
        ).toBeGreaterThanOrEqual(3);
      }
      expect(
        contrast(properties["--theme-selection-text"]!, properties["--theme-selection"]!),
        `${theme.name} selection text`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  /**
   * The subdued tiers are decorative and carry no contrast floor. The only
   * invariant is that none of them outshouts body text — palettes are free to
   * order the tiers among themselves however their character demands (Dracula
   * puts its signature comment blue at surface-500, above surface-400).
   */
  it("keeps the subdued tiers quieter than body text", () => {
    const subdued = [
      "--theme-text-tertiary",
      "--theme-text-quiet",
      "--theme-text-faint",
    ] as const;

    for (const theme of allThemes) {
      const properties = theme.properties as Record<string, string>;
      const canvas = properties["--theme-canvas"]!;
      const secondary = contrast(properties["--theme-text-secondary"]!, canvas);

      expect(
        contrast(properties["--theme-text"]!, canvas),
        `${theme.name} primary text should lead the ladder`,
      ).toBeGreaterThanOrEqual(secondary);

      for (const role of subdued) {
        expect(
          contrast(properties[role]!, canvas),
          `${theme.name} ${role} should be quieter than secondary text`,
        ).toBeLessThanOrEqual(secondary);
      }
    }
  });

  /**
   * Light ramps mirror the dark ones, which is right for backgrounds and wrong
   * for text: a mirrored mid-step lands far closer to paper than to ink, so the
   * subdued tiers used to collapse to 1.2-2.6:1. The contract now derives them
   * against contrast targets instead of reading fixed ramp steps.
   */
  describe("light themes", () => {
    const backgroundRoles = [
      "--theme-canvas",
      "--theme-chrome",
      "--theme-header",
      "--theme-pane",
      "--theme-pane-muted",
      "--theme-card",
      "--theme-card-hover",
    ] as const;

    const lightThemes = allThemes.filter(
      (theme) =>
        luminance((theme.properties as Record<string, string>)["--theme-canvas"]!) > 0.5,
    );

    function darkestSurface(properties: Record<string, string>): string {
      return backgroundRoles
        .map((role) => properties[role]!)
        .reduce((darkest, background) =>
          luminance(background) < luminance(darkest) ? background : darkest,
        );
    }

    it("ships a light half of the catalog", () => {
      expect(lightThemes.length).toBeGreaterThanOrEqual(13);
    });

    it("keeps the subdued ladder ordered and legible on the least forgiving surface", () => {
      for (const theme of lightThemes) {
        const properties = theme.properties as Record<string, string>;
        const worst = darkestSurface(properties);
        const step = (role: string) => contrast(properties[role]!, worst);

        const text = step("--theme-text");
        const secondary = step("--theme-text-secondary");
        const tertiary = step("--theme-text-tertiary");
        const quiet = step("--theme-text-quiet");
        const faint = step("--theme-text-faint");

        expect(text, `${theme.name} text leads secondary`).toBeGreaterThanOrEqual(secondary);
        expect(secondary, `${theme.name} secondary leads tertiary`).toBeGreaterThan(tertiary);
        expect(tertiary, `${theme.name} tertiary leads quiet`).toBeGreaterThan(quiet);
        expect(quiet, `${theme.name} quiet leads faint`).toBeGreaterThan(faint);

        expect(quiet, `${theme.name} quiet is the most-used dim tier`).toBeGreaterThanOrEqual(4);
        expect(faint, `${theme.name} faint stays visible`).toBeGreaterThanOrEqual(2.9);
      }
    });

    it("darkens syntax tokens so code is readable on light paper", () => {
      const tokens = [
        "--syn-fg",
        "--syn-comment",
        "--syn-keyword",
        "--syn-string",
        "--syn-number",
        "--syn-function",
        "--syn-type",
        "--syn-attr",
        "--syn-operator",
        "--syn-meta",
        "--syn-punctuation",
        "--syn-title",
        "--syn-addition-fg",
        "--syn-deletion-fg",
      ] as const;

      for (const theme of lightThemes) {
        const properties = theme.properties as Record<string, string>;
        for (const token of tokens) {
          expect(
            contrast(properties[token]!, properties["--syn-bg"]!),
            `${theme.name} ${token} on syntax background`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
    });
  });

  /**
   * Dark ramps already fan out across surface 300-600, so the derivation must
   * not reach them. These are the stops the light branch would have replaced.
   */
  it("leaves dark themes on the verbatim ramp steps", () => {
    const darkThemes = allThemes.filter(
      (theme) =>
        luminance((theme.properties as Record<string, string>)["--theme-canvas"]!) <= 0.5,
    );
    expect(darkThemes.length).toBeGreaterThanOrEqual(13);

    for (const theme of darkThemes) {
      const properties = theme.properties as Record<string, string>;
      expect(properties["--theme-text-tertiary"], theme.name).toBe(
        properties["--color-surface-400"],
      );
      expect(properties["--theme-text-quiet"], theme.name).toBe(
        properties["--color-surface-500"],
      );
      expect(properties["--theme-text-faint"], theme.name).toBe(
        properties["--color-surface-600"],
      );
      const unchanged: Array<[string, string]> = [
        ["--syn-bg", "--color-surface-900"],
        ["--syn-comment", "--color-surface-400"],
        ["--syn-meta", "--color-surface-400"],
        ["--syn-operator", "--color-surface-300"],
        ["--syn-punctuation", "--color-surface-300"],
        ["--syn-keyword", "--color-secondary-300"],
        ["--syn-string", "--color-tertiary-300"],
        ["--syn-number", "--color-warning-300"],
        ["--syn-function", "--color-primary-300"],
        ["--syn-type", "--color-tertiary-200"],
        ["--syn-attr", "--color-secondary-200"],
        ["--syn-title", "--color-tertiary-200"],
        ["--syn-addition-fg", "--color-success-300"],
        ["--syn-deletion-fg", "--color-error-300"],
      ];
      for (const [token, step] of unchanged) {
        expect(properties[token], `${theme.name} ${token}`).toBe(properties[step]);
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
