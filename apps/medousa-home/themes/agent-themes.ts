import { buildDarkTheme, buildTheme } from "./theme-utils";
import {
  darkSurfacesCaduceus,
  darkSurfacesEmber,
  darkSurfacesHearth,
  lightSurfacesCaduceus,
  lightSurfacesEmber,
  lightSurfacesHearth,
} from "./surface-scales";

/** Caduceus — serpentine ink, soft jade (apothecary desk). */
export const caduceusDarkTheme = buildDarkTheme("caduceus-dark", darkSurfacesCaduceus, {
  primary: "126 180 148",
  secondary: "140 168 176",
  tertiary: "148 160 100",
  onPrimary: "12 20 16",
  onSurface: "232 240 234",
  fontBase: "206 218 208",
});

export const caduceusLightTheme = buildTheme("caduceus-light", lightSurfacesCaduceus, {
  primary: "62 118 88",
  secondary: "70 108 118",
  tertiary: "108 120 64",
  onPrimary: "255 255 255",
  onSurface: "18 28 22",
  fontBase: "18 28 22",
});

/** Ember — banked coals: cool ash + soft rose-copper. */
export const emberDarkTheme = buildDarkTheme("ember-dark", darkSurfacesEmber, {
  primary: "204 118 98",
  secondary: "160 148 144",
  tertiary: "196 140 88",
  onPrimary: "255 255 255",
  onSurface: "244 234 228",
  fontBase: "222 204 194",
});

export const emberLightTheme = buildTheme("ember-light", lightSurfacesEmber, {
  primary: "166 78 62",
  secondary: "120 100 94",
  tertiary: "156 108 64",
  onPrimary: "255 255 255",
  onSurface: "28 20 18",
  fontBase: "28 20 18",
});

/** Hearth — evening cocoa, dusty rose (reading nook). */
export const hearthDarkTheme = buildDarkTheme("hearth-dark", darkSurfacesHearth, {
  primary: "186 122 108",
  secondary: "168 156 144",
  tertiary: "176 140 96",
  onPrimary: "255 255 255",
  onSurface: "242 236 228",
  fontBase: "222 210 198",
});

export const hearthLightTheme = buildTheme("hearth-light", lightSurfacesHearth, {
  primary: "156 92 80",
  secondary: "110 96 86",
  tertiary: "140 108 72",
  onPrimary: "255 255 255",
  onSurface: "32 26 22",
  fontBase: "32 26 22",
});
