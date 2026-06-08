export type ColorThemeId = "medousa" | "black-lily";

export interface ColorThemeOption {
  id: ColorThemeId;
  label: string;
  tagline: string;
  swatches: [string, string, string];
}

export const COLOR_THEME_OPTIONS: ColorThemeOption[] = [
  {
    id: "medousa",
    label: "Obsidian",
    tagline: "Near-black canvas, violet accent — Medousa default",
    swatches: ["#101018", "#7C3AED", "#282836"],
  },
  {
    id: "black-lily",
    label: "Black Lily",
    tagline: "Void black, lily pink & hex purple — calm goth accent",
    swatches: ["#0D0A0F", "#E8559A", "#1E1427"],
  },
];

export const DEFAULT_COLOR_THEME: ColorThemeId = "medousa";

export function isColorThemeId(value: string | null | undefined): value is ColorThemeId {
  return value === "medousa" || value === "black-lily";
}
