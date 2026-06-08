import { buildDarkTheme, buildTheme } from "./theme-utils";
import {
  darkSurfacesApple,
  darkSurfacesMidnight,
  lightSurfacesApple,
  lightSurfacesMidnight,
} from "./surface-scales";

/** SF Blue — clean system light/dark like iOS Settings. */
export const cupertinoLightTheme = buildTheme("cupertino-light", lightSurfacesApple, {
  primary: "0 122 255",
  secondary: "90 200 250",
  tertiary: "175 82 222",
  onSurface: "28 28 30",
  fontBase: "28 28 30",
});

export const cupertinoDarkTheme = buildDarkTheme("cupertino-dark", darkSurfacesApple, {
  primary: "10 132 255",
  secondary: "100 210 255",
  tertiary: "191 90 242",
  onSurface: "255 255 255",
  fontBase: "242 242 247",
});

/** Warm neutral gray — calm, almost monochrome. */
export const graphiteLightTheme = buildTheme("graphite-light", lightSurfacesApple, {
  primary: "72 72 74",
  secondary: "0 122 255",
  tertiary: "142 142 147",
  onSurface: "28 28 30",
  fontBase: "28 28 30",
});

export const graphiteDarkTheme = buildDarkTheme("graphite-dark", darkSurfacesApple, {
  primary: "174 174 178",
  secondary: "10 132 255",
  tertiary: "142 142 147",
  onSurface: "255 255 255",
  fontBase: "229 229 234",
});

/** Deep navy canvas — night mode with a cool blue accent. */
export const midnightLightTheme = buildTheme("midnight-light", lightSurfacesMidnight, {
  primary: "0 61 165",
  secondary: "0 122 255",
  tertiary: "88 86 214",
  onSurface: "18 24 42",
  fontBase: "18 24 42",
});

export const midnightDarkTheme = buildDarkTheme("midnight-dark", darkSurfacesMidnight, {
  primary: "10 132 255",
  secondary: "100 210 255",
  tertiary: "94 92 230",
  onSurface: "236 240 252",
  fontBase: "210 218 240",
});
