import type { ColorThemeId } from "$lib/types/colorThemes";
import {
  COLOR_THEME_OPTIONS,
  DEFAULT_COLOR_THEME,
  isColorThemeId,
} from "$lib/types/colorThemes";
import type { EnvironmentSpec } from "$lib/types/environment";

export interface HostThemeTokens {
  fg: string;
  muted: string;
  accent: string;
  surface: string;
  brand: string;
}

export interface ResolvedEnvironmentTheme {
  colorThemeId: ColorThemeId;
  brandColor: string | null;
  tagline: string | null;
  tokens: HostThemeTokens;
  paletteLabel: string;
}

const DEFAULT_FG_DARK = "#f4f4f5";
const DEFAULT_FG_LIGHT = "#18181b";
const DEFAULT_MUTED_DARK = "#a1a1aa";
const DEFAULT_MUTED_LIGHT = "#52525b";

export function resolveEnvironmentTheme(
  spec: EnvironmentSpec | null | undefined,
  workshopColorThemeId: string | null | undefined,
  workshopBrandColor: string | null | undefined,
  isDark: boolean,
): ResolvedEnvironmentTheme {
  const envTheme = spec?.theme;
  const colorThemeId = pickColorThemeId(
    envTheme?.colorThemeId,
    workshopColorThemeId,
  );
  const brandColor =
    normalizeBrand(envTheme?.brandColor) ?? normalizeBrand(workshopBrandColor);
  const tagline = envTheme?.tagline?.trim() || null;
  const option =
    COLOR_THEME_OPTIONS.find((entry) => entry.id === colorThemeId) ??
    COLOR_THEME_OPTIONS.find((entry) => entry.id === DEFAULT_COLOR_THEME)!;
  const tokens = hostTokensFromSwatches(option.swatches, brandColor, isDark);

  return {
    colorThemeId,
    brandColor,
    tagline,
    tokens,
    paletteLabel: option.label,
  };
}

function pickColorThemeId(
  envId: string | null | undefined,
  workshopId: string | null | undefined,
): ColorThemeId {
  if (envId && isColorThemeId(envId)) return envId;
  if (workshopId && isColorThemeId(workshopId)) return workshopId;
  return DEFAULT_COLOR_THEME;
}

function normalizeBrand(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  if (/^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(trimmed)) return trimmed;
  return null;
}

function hostTokensFromSwatches(
  swatches: [string, string, string],
  brandColor: string | null,
  isDark: boolean,
): HostThemeTokens {
  const [_bg, accent, surface] = swatches;
  return {
    fg: isDark ? DEFAULT_FG_DARK : DEFAULT_FG_LIGHT,
    muted: isDark ? DEFAULT_MUTED_DARK : DEFAULT_MUTED_LIGHT,
    accent,
    surface,
    brand: brandColor ?? accent,
  };
}

/** Legacy helper for tests that only pass light/dark. */
export function legacyHostThemeTokens(isDark: boolean): HostThemeTokens {
  return resolveEnvironmentTheme(null, null, null, isDark).tokens;
}

export function environmentThemeLabel(themeId: ColorThemeId): string {
  return (
    COLOR_THEME_OPTIONS.find((entry) => entry.id === themeId)?.label ?? themeId
  );
}
