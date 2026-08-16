import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { CustomThemeConfig } from "@skeletonlabs/tw-plugin";

export type ThemeCatalogEntry = {
  name: string;
  tokenPath: string;
};

export function themeTokenPath(name: string): string {
  return `/themes/${name}.css`;
}

export function themePropertiesToCss(theme: CustomThemeConfig): string {
  const body = Object.entries(theme.properties)
    .map(([key, value]) => `  ${key}: ${String(value)};`)
    .join("\n");
  return `[data-theme="${theme.name}"] {\n${body}\n}\n`;
}

export function themeCatalogEntries(
  themes: readonly CustomThemeConfig[],
): ThemeCatalogEntry[] {
  return themes.map((theme) => ({
    name: theme.name,
    tokenPath: themeTokenPath(theme.name),
  }));
}

export function writeThemeCssFiles(
  directory: string,
  themes: readonly CustomThemeConfig[],
): string[] {
  mkdirSync(directory, { recursive: true });
  const written: string[] = [];
  for (const theme of themes) {
    const file = join(directory, `${theme.name}.css`);
    writeFileSync(file, themePropertiesToCss(theme));
    written.push(file);
  }
  return written;
}
