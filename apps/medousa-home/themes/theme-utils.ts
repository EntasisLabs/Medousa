import type { CustomThemeConfig } from "@skeletonlabs/tw-plugin";

type ThemeProps = CustomThemeConfig["properties"];

const sharedMeta = {
  "--theme-border-base": "1px",
  "--theme-font-family-base": "-apple-system, BlinkMacSystemFont, system-ui, sans-serif",
  "--theme-font-family-heading": "-apple-system, BlinkMacSystemFont, system-ui, sans-serif",
  "--theme-rounded-base": "9999px",
  "--theme-rounded-container": "10px",
} as const;

const statusLight = {
  "--color-error-500": "255 59 48",
  "--color-success-500": "52 199 89",
  "--color-warning-500": "255 149 0",
  "--on-error": "255 255 255",
  "--on-success": "255 255 255",
  "--on-warning": "255 255 255",
} as const;

const statusDark = statusLight;

export function buildTheme(
  name: string,
  surfaces: Record<string, string>,
  accents: {
    primary: string;
    secondary: string;
    tertiary?: string;
    onPrimary?: string;
    onSurface: string;
    fontBase: string;
  },
  extras: ThemeProps = {},
): CustomThemeConfig {
  return {
    name,
    properties: {
      ...surfaces,
      "--color-primary-50": accents.primary,
      "--color-primary-100": accents.primary,
      "--color-primary-200": accents.primary,
      "--color-primary-300": accents.primary,
      "--color-primary-400": accents.primary,
      "--color-primary-500": accents.primary,
      "--color-primary-600": accents.primary,
      "--color-primary-700": accents.primary,
      "--color-primary-800": accents.primary,
      "--color-primary-900": accents.primary,
      "--color-secondary-50": accents.secondary,
      "--color-secondary-100": accents.secondary,
      "--color-secondary-200": accents.secondary,
      "--color-secondary-300": accents.secondary,
      "--color-secondary-400": accents.secondary,
      "--color-secondary-500": accents.secondary,
      "--color-secondary-600": accents.secondary,
      "--color-secondary-700": accents.secondary,
      "--color-secondary-800": accents.secondary,
      "--color-secondary-900": accents.secondary,
      "--color-tertiary-500": accents.tertiary ?? accents.secondary,
      "--on-primary": accents.onPrimary ?? "255 255 255",
      "--on-secondary": "255 255 255",
      "--on-surface": accents.onSurface,
      "--on-tertiary": accents.onSurface,
      "--theme-font-color-base": accents.fontBase,
      "--theme-font-color-dark": accents.fontBase,
      ...statusLight,
      ...sharedMeta,
      ...extras,
    },
  };
}

export function buildDarkTheme(
  name: string,
  surfaces: Record<string, string>,
  accents: Parameters<typeof buildTheme>[2],
  extras?: ThemeProps,
): CustomThemeConfig {
  return buildTheme(name, surfaces, accents, { ...statusDark, ...extras });
}
