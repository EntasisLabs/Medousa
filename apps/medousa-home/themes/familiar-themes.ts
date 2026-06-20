import { buildDarkTheme, buildTheme } from "./theme-utils";
import {
  darkSurfacesCatppuccin,
  darkSurfacesGithub,
  darkSurfacesOneDark,
  darkSurfacesTokyoNight,
  lightSurfacesCatppuccin,
  lightSurfacesGithub,
  lightSurfacesOneDark,
  lightSurfacesTokyoDay,
  darkSurfacesDracula,
  darkSurfacesNord,
  darkSurfacesSolarized,
  lightSurfacesDracula,
  lightSurfacesNord,
  lightSurfacesSolarized,
} from "./surface-scales";

/** Atom One Dark Pro — #282c34 canvas, blue accent. */
export const oneDarkTheme = buildDarkTheme("one-dark", darkSurfacesOneDark, {
  primary: "97 175 239",
  secondary: "198 120 221",
  tertiary: "152 195 121",
  onSurface: "171 178 191",
  fontBase: "171 178 191",
});

export const oneDarkLightTheme = buildTheme("one-dark-light", lightSurfacesOneDark, {
  primary: "64 120 242",
  secondary: "152 95 182",
  tertiary: "80 121 66",
  onSurface: "56 58 66",
  fontBase: "56 58 66",
});

/** Catppuccin Mocha / Latte — community pastel standard. */
export const catppuccinMochaTheme = buildDarkTheme(
  "catppuccin-mocha",
  darkSurfacesCatppuccin,
  {
    primary: "203 166 247",
    secondary: "137 180 250",
    tertiary: "166 227 161",
    onSurface: "205 214 244",
    fontBase: "205 214 244",
  },
);

export const catppuccinLatteTheme = buildTheme(
  "catppuccin-latte",
  lightSurfacesCatppuccin,
  {
    primary: "136 57 239",
    secondary: "30 102 245",
    tertiary: "64 160 43",
    onSurface: "76 79 105",
    fontBase: "76 79 105",
  },
);

/** Tokyo Night / Day — indigo night desk. */
export const tokyoNightTheme = buildDarkTheme("tokyo-night", darkSurfacesTokyoNight, {
  primary: "122 162 247",
  secondary: "187 154 247",
  tertiary: "158 206 106",
  onSurface: "169 177 214",
  fontBase: "169 177 214",
});

export const tokyoDayTheme = buildTheme("tokyo-day", lightSurfacesTokyoDay, {
  primary: "42 95 198",
  secondary: "120 88 196",
  tertiary: "56 124 43",
  onSurface: "52 58 82",
  fontBase: "52 58 82",
});

/** GitHub Dark / Light — marketplace default for millions of devs. */
export const githubDarkTheme = buildDarkTheme("github-dark", darkSurfacesGithub, {
  primary: "88 166 255",
  secondary: "163 113 247",
  tertiary: "63 185 80",
  onSurface: "230 237 243",
  fontBase: "230 237 243",
});

export const githubLightTheme = buildTheme("github-light", lightSurfacesGithub, {
  primary: "9 105 218",
  secondary: "130 80 223",
  tertiary: "26 127 55",
  onSurface: "36 41 47",
  fontBase: "36 41 47",
});

/** Dracula — #282a36 canvas, purple/pink accents. */
export const draculaTheme = buildDarkTheme("dracula", darkSurfacesDracula, {
  primary: "189 147 249",
  secondary: "255 121 198",
  tertiary: "139 233 253",
  onSurface: "248 248 242",
  fontBase: "248 248 242",
});

export const draculaLightTheme = buildTheme("dracula-light", lightSurfacesDracula, {
  primary: "98 54 178",
  secondary: "196 54 130",
  tertiary: "0 142 170",
  onSurface: "68 71 90",
  fontBase: "68 71 90",
});

/** Nord — polar night #2e3440, frost blue accent. */
export const nordTheme = buildDarkTheme("nord", darkSurfacesNord, {
  primary: "136 192 208",
  secondary: "129 161 193",
  tertiary: "163 190 140",
  onSurface: "216 222 233",
  fontBase: "216 222 233",
});

export const nordLightTheme = buildTheme("nord-light", lightSurfacesNord, {
  primary: "62 139 168",
  secondary: "94 129 172",
  tertiary: "96 138 92",
  onSurface: "46 52 64",
  fontBase: "46 52 64",
});

/** Solarized Dark / Light — Ethan Schoonover precision palette. */
export const solarizedDarkTheme = buildDarkTheme(
  "solarized-dark",
  darkSurfacesSolarized,
  {
    primary: "38 139 210",
    secondary: "211 54 130",
    tertiary: "133 153 0",
    onSurface: "131 148 150",
    fontBase: "131 148 150",
  },
);

export const solarizedLightTheme = buildTheme(
  "solarized-light",
  lightSurfacesSolarized,
  {
    primary: "38 139 210",
    secondary: "211 54 130",
    tertiary: "133 153 0",
    onSurface: "7 54 66",
    fontBase: "7 54 66",
  },
);
