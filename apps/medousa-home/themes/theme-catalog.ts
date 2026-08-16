import type { CustomThemeConfig } from "@skeletonlabs/tw-plugin";
import { medousaTheme } from "../medousa-theme";
import { blackLilyTheme } from "../black-lily-theme";
import { medousaLightTheme, blackLilyLightTheme } from "./light-themes";
import {
  caduceusDarkTheme,
  caduceusLightTheme,
  emberDarkTheme,
  emberLightTheme,
  hearthDarkTheme,
  hearthLightTheme,
} from "./agent-themes";
import {
  cupertinoDarkTheme,
  cupertinoLightTheme,
  graphiteDarkTheme,
  graphiteLightTheme,
  midnightDarkTheme,
  midnightLightTheme,
} from "./apple-themes";
import {
  catppuccinLatteTheme,
  catppuccinMochaTheme,
  draculaLightTheme,
  draculaTheme,
  githubDarkTheme,
  githubLightTheme,
  nordLightTheme,
  nordTheme,
  oneDarkLightTheme,
  oneDarkTheme,
  solarizedDarkTheme,
  solarizedLightTheme,
  tokyoDayTheme,
  tokyoNightTheme,
} from "./familiar-themes";
import { brandMarkThemes } from "./brand-mark-themes";

/** Every shipped theme passes through the same contract before entering Skeleton. */
export const allThemes: CustomThemeConfig[] = [
  medousaTheme,
  medousaLightTheme,
  blackLilyTheme,
  blackLilyLightTheme,
  caduceusDarkTheme,
  caduceusLightTheme,
  emberDarkTheme,
  emberLightTheme,
  hearthDarkTheme,
  hearthLightTheme,
  cupertinoLightTheme,
  cupertinoDarkTheme,
  graphiteLightTheme,
  graphiteDarkTheme,
  midnightLightTheme,
  midnightDarkTheme,
  oneDarkTheme,
  oneDarkLightTheme,
  catppuccinMochaTheme,
  catppuccinLatteTheme,
  tokyoNightTheme,
  tokyoDayTheme,
  githubDarkTheme,
  githubLightTheme,
  draculaTheme,
  draculaLightTheme,
  nordTheme,
  nordLightTheme,
  solarizedDarkTheme,
  solarizedLightTheme,
  ...brandMarkThemes,
];

export { themeCatalogEntries, themeTokenPath } from "./theme-css";
