import type { ColorThemeId } from "$lib/types/colorThemes";

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
};

export function resolveSkeletonThemeName(
  themeId: ColorThemeId,
  darkMode: boolean,
): string {
  const pair = SKELETON_THEME_NAMES[themeId];
  return darkMode ? pair.dark : pair.light;
}

/** Inline boot script in app.html — keep in sync with ColorThemeId. */
export const COLOR_THEME_IDS = [
  "medousa",
  "black-lily",
  "cupertino",
  "graphite",
  "midnight",
] as const;

export function isStoredColorThemeId(value: string | null | undefined): value is ColorThemeId {
  return (
    value === "medousa" ||
    value === "black-lily" ||
    value === "cupertino" ||
    value === "graphite" ||
    value === "midnight"
  );
}
