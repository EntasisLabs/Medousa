/** Single source of truth for color palettes, Skeleton names, and FOUC boot script. */

export type ColorThemeGroup = "workshop" | "apple" | "familiar";

export type ColorThemeId =
  | "medousa"
  | "black-lily"
  | "cupertino"
  | "graphite"
  | "midnight"
  | "one-dark"
  | "catppuccin"
  | "tokyo-night"
  | "github"
  | "dracula"
  | "nord"
  | "solarized";

export interface ColorThemeOption {
  id: ColorThemeId;
  label: string;
  tagline: string;
  swatches: [string, string, string];
  group: ColorThemeGroup;
}

/** Skeleton `data-theme` names resolved from appearance mode + palette id. */
export const SKELETON_THEME_NAMES: Record<
  ColorThemeId,
  { dark: string; light: string }
> = {
  medousa: { dark: "medousa", light: "medousa-light" },
  "black-lily": { dark: "black-lily", light: "black-lily-light" },
  cupertino: { dark: "cupertino-dark", light: "cupertino-light" },
  graphite: { dark: "graphite-dark", light: "graphite-light" },
  midnight: { dark: "midnight-dark", light: "midnight-light" },
  "one-dark": { dark: "one-dark", light: "one-dark-light" },
  catppuccin: { dark: "catppuccin-mocha", light: "catppuccin-latte" },
  "tokyo-night": { dark: "tokyo-night", light: "tokyo-day" },
  github: { dark: "github-dark", light: "github-light" },
  dracula: { dark: "dracula", light: "dracula-light" },
  nord: { dark: "nord", light: "nord-light" },
  solarized: { dark: "solarized-dark", light: "solarized-light" },
};

export const COLOR_THEME_IDS = Object.keys(SKELETON_THEME_NAMES) as ColorThemeId[];

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
  {
    id: "one-dark",
    label: "One Dark",
    tagline: "Atom One Dark Pro — balanced contrast for long sessions",
    swatches: ["#282C34", "#61AFEF", "#21252B"],
    group: "familiar",
  },
  {
    id: "catppuccin",
    label: "Catppuccin",
    tagline: "Soothing pastels — Mocha dark, Latte light",
    swatches: ["#1E1E2E", "#CBA6F7", "#EFF1F5"],
    group: "familiar",
  },
  {
    id: "tokyo-night",
    label: "Tokyo Night",
    tagline: "Deep indigo night desk — popular VS Code palette",
    swatches: ["#1A1B26", "#7AA2F7", "#E6EEF7"],
    group: "familiar",
  },
  {
    id: "github",
    label: "GitHub",
    tagline: "GitHub Dark / Light — the most-installed editor theme",
    swatches: ["#0D1117", "#58A6FF", "#FFFFFF"],
    group: "familiar",
  },
  {
    id: "dracula",
    label: "Dracula",
    tagline: "Purple-pink gothic — ~10M VS Code installs",
    swatches: ["#282A36", "#BD93F9", "#FF79C6"],
    group: "familiar",
  },
  {
    id: "nord",
    label: "Nord",
    tagline: "Arctic blue-gray — calm polar night palette",
    swatches: ["#2E3440", "#88C0D0", "#ECEFF4"],
    group: "familiar",
  },
  {
    id: "solarized",
    label: "Solarized",
    tagline: "Ethan Schoonover classic — precision dark & light",
    swatches: ["#002B36", "#268BD2", "#FDF6E3"],
    group: "familiar",
  },
];

export const DEFAULT_COLOR_THEME: ColorThemeId = "medousa";

export const COLOR_THEME_GROUP_LABELS: Record<ColorThemeGroup, string> = {
  workshop: "Workshop",
  apple: "Apple",
  familiar: "Editor familiars",
};

export const COLOR_THEME_GROUPS: ColorThemeGroup[] = ["workshop", "familiar", "apple"];

export function isColorThemeId(value: string | null | undefined): value is ColorThemeId {
  return COLOR_THEME_IDS.includes(value as ColorThemeId);
}

export function resolveSkeletonThemeName(
  themeId: ColorThemeId,
  darkMode: boolean,
): string {
  const pair = SKELETON_THEME_NAMES[themeId];
  return darkMode ? pair.dark : pair.light;
}

const DARK_MODE_KEY = "medousa-home-dark-mode";
const COLOR_THEME_KEY = "medousa-home-color-theme";

/** Inline boot script for app.html — generated from this registry only. */
export function buildThemeBootScript(): string {
  return `(function () {
  try {
    var dark = localStorage.getItem(${JSON.stringify(DARK_MODE_KEY)}) !== "0";
    var stored = localStorage.getItem(${JSON.stringify(COLOR_THEME_KEY)});
    var ids = ${JSON.stringify(COLOR_THEME_IDS)};
    var themeId = ids.indexOf(stored) >= 0 ? stored : ${JSON.stringify(DEFAULT_COLOR_THEME)};
    var names = ${JSON.stringify(SKELETON_THEME_NAMES)};
    var skeletonTheme = names[themeId][dark ? "dark" : "light"];
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.dataset.theme = skeletonTheme;
    if (
      window.__TAURI_INTERNALS__ &&
      /iPhone|iPad|iPod/i.test(navigator.userAgent)
    ) {
      document.documentElement.dataset.nativeShell = "ios";
    }
  } catch (_) {}
})();`;
}
