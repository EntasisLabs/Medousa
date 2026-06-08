export type ColorThemeId =
  | "medousa"
  | "black-lily"
  | "cupertino"
  | "graphite"
  | "midnight";

export interface ColorThemeOption {
  id: ColorThemeId;
  label: string;
  tagline: string;
  swatches: [string, string, string];
  /** Apple-style group label in settings */
  group: "workshop" | "apple";
}

export const COLOR_THEME_OPTIONS: ColorThemeOption[] = [
  {
    id: "medousa",
    label: "Obsidian",
    tagline: "Near-black canvas, violet accent — Medousa default",
    swatches: ["#101018", "#7C3AED", "#282836"],
    group: "workshop",
  },
  {
    id: "black-lily",
    label: "Black Lily",
    tagline: "Void black, lily pink & hex purple — calm goth accent",
    swatches: ["#0D0A0F", "#E8559A", "#1E1427"],
    group: "workshop",
  },
  {
    id: "cupertino",
    label: "Cupertino",
    tagline: "SF Blue on system gray — iOS calm default",
    swatches: ["#F5F5F7", "#007AFF", "#1C1C1E"],
    group: "apple",
  },
  {
    id: "graphite",
    label: "Graphite",
    tagline: "Neutral gray, whisper of blue — minimal & quiet",
    swatches: ["#F2F2F7", "#48484A", "#2C2C2E"],
    group: "apple",
  },
  {
    id: "midnight",
    label: "Midnight",
    tagline: "Deep navy canvas, cool blue accent — night desk",
    swatches: ["#080C16", "#0A84FF", "#0F1423"],
    group: "apple",
  },
];

export const DEFAULT_COLOR_THEME: ColorThemeId = "medousa";

export function isColorThemeId(value: string | null | undefined): value is ColorThemeId {
  return (
    value === "medousa" ||
    value === "black-lily" ||
    value === "cupertino" ||
    value === "graphite" ||
    value === "midnight"
  );
}
