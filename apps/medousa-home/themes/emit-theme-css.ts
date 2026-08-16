import { join } from "node:path";
import { allThemes } from "./theme-catalog";
import { themePropertiesToCss, writeThemeCssFiles } from "./theme-css";

export function emitSelectedThemeSheets(
  homeRoot: string,
): Array<{ name: string; source: string }> {
  writeThemeCssFiles(join(homeRoot, "static/themes"), allThemes);
  return allThemes.map((theme) => ({
    name: theme.name,
    source: themePropertiesToCss(theme),
  }));
}
