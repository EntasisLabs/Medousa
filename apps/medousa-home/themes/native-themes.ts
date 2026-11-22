import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import type { CustomThemeConfig } from "@skeletonlabs/tw-plugin";
import {
  COLOR_THEME_OPTIONS,
  DEFAULT_COLOR_THEME,
  SKELETON_THEME_NAMES,
} from "../src/lib/theme/themeRegistry";
import { allThemes } from "./theme-catalog";

/**
 * Resolved theme data for the native (RNDR) shell. Same contract the CSS
 * sheets carry, with colors as numbers so the native app never parses CSS.
 * Native owns how these roles map onto its toolkit tokens.
 */
export const NATIVE_THEMES_VERSION = 1;

type Rgb = [number, number, number];

export interface NativeThemeVariant {
  /** `--theme-*` color roles, keyed without the prefix (`canvas`, `text-quiet`). */
  roles: Record<string, Rgb>;
  /** `--syn-*` code colors, keyed without the prefix. */
  syntax: Record<string, Rgb>;
  /** `--color-surface-{50..950}` and `--color-primary-{50..950}` ramps. */
  surface: Record<string, Rgb>;
  primary: Record<string, Rgb>;
  onPrimary: Rgb;
  chromeAlpha: number;
  paneAlpha: number;
  /** Logical points at a 16pt root. */
  controlRadius: number;
  containerRadius: number;
}

export interface NativeThemeCatalog {
  version: number;
  defaultTheme: string;
  themes: Array<{
    id: string;
    label: string;
    dark: NativeThemeVariant;
    light: NativeThemeVariant;
  }>;
}

function rgb(value: unknown): Rgb | null {
  const parts = String(value).trim().split(/\s+/).map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part))) return null;
  return parts as Rgb;
}

function points(value: unknown): number {
  const text = String(value).trim();
  const amount = Number.parseFloat(text);
  if (!Number.isFinite(amount)) throw new Error(`not a length: ${text}`);
  return text.endsWith("rem") ? amount * 16 : amount;
}

function prefixed(properties: Record<string, unknown>, prefix: string): Record<string, Rgb> {
  const out: Record<string, Rgb> = {};
  for (const [key, value] of Object.entries(properties)) {
    if (!key.startsWith(prefix)) continue;
    const color = rgb(value);
    if (color) out[key.slice(prefix.length)] = color;
  }
  return out;
}

function variant(theme: CustomThemeConfig): NativeThemeVariant {
  const properties = theme.properties as Record<string, unknown>;
  const onPrimary = rgb(properties["--on-primary"]);
  if (!onPrimary) throw new Error(`${theme.name}: --on-primary is not rgb`);
  return {
    roles: prefixed(properties, "--theme-"),
    syntax: prefixed(properties, "--syn-"),
    surface: prefixed(properties, "--color-surface-"),
    primary: prefixed(properties, "--color-primary-"),
    onPrimary,
    chromeAlpha: Number(properties["--theme-chrome-alpha"]),
    paneAlpha: Number(properties["--theme-pane-alpha"]),
    controlRadius: points(properties["--theme-control-radius"]),
    containerRadius: points(properties["--theme-container-radius"]),
  };
}

export function nativeThemeCatalog(
  themes: readonly CustomThemeConfig[] = allThemes,
): NativeThemeCatalog {
  const byName = new Map(themes.map((theme) => [theme.name, theme]));
  const resolve = (name: string) => {
    const theme = byName.get(name);
    if (!theme) throw new Error(`theme ${name} is not in the catalog`);
    return variant(theme);
  };
  return {
    version: NATIVE_THEMES_VERSION,
    defaultTheme: DEFAULT_COLOR_THEME,
    themes: COLOR_THEME_OPTIONS.map((option) => ({
      id: option.id,
      label: option.label,
      dark: resolve(SKELETON_THEME_NAMES[option.id].dark),
      light: resolve(SKELETON_THEME_NAMES[option.id].light),
    })),
  };
}

export function nativeThemesJson(): string {
  return `${JSON.stringify(nativeThemeCatalog())}\n`;
}

/** Write the catalog, or with `check` report whether the file is current. */
export function syncNativeThemes(file: string, check = false): boolean {
  const next = nativeThemesJson();
  let current = "";
  try {
    current = readFileSync(file, "utf8");
  } catch {
    // Missing file is stale.
  }
  if (current === next) return true;
  if (check) return false;
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, next);
  return true;
}
