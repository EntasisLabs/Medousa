import { buildDarkTheme, buildTheme } from "./theme-utils";
import {
  darkSurfacesMedousaBrand,
  lightSurfacesMedousaBrand,
  tintSurfaceScale,
} from "./surface-scales";
import type { ThemePersonality } from "./theme-contract";

type AccentSpec = {
  primary: string;
  secondary: string;
  tertiary?: string;
  surfaceTint: string;
  surfaceStrength?: number;
  personality?: ThemePersonality;
  darkOnPrimary?: string;
  lightOnPrimary?: string;
};

function markPair(name: string, accent: AccentSpec) {
  const tertiary = accent.tertiary ?? accent.secondary;
  const personality: ThemePersonality = {
    roles: {
      focus: accent.secondary,
      decorative: tertiary,
      ...accent.personality?.roles,
    },
    syntax: accent.personality?.syntax,
    charts: accent.personality?.charts,
    effects: {
      glow: accent.primary,
      gradientA: accent.primary,
      gradientB: accent.secondary,
      gradientC: tertiary,
      glowStrength: "0.17",
      ...accent.personality?.effects,
    },
    shape: accent.personality?.shape,
  };
  return [
    buildDarkTheme(
      `${name}-dark`,
      tintSurfaceScale(
        darkSurfacesMedousaBrand,
        accent.surfaceTint,
        "dark",
        accent.surfaceStrength,
      ),
      {
        primary: accent.primary,
        secondary: accent.secondary,
        tertiary: accent.tertiary,
        onPrimary: accent.darkOnPrimary ?? "255 255 255",
        onSurface: "242 239 230",
        fontBase: "226 223 216",
      },
      undefined,
      personality,
    ),
    buildTheme(
      `${name}-light`,
      tintSurfaceScale(
        lightSurfacesMedousaBrand,
        accent.surfaceTint,
        "light",
        accent.surfaceStrength,
      ),
      {
        primary: accent.primary,
        secondary: accent.secondary,
        tertiary: accent.tertiary,
        onPrimary: accent.lightOnPrimary ?? "255 255 255",
        onSurface: "0 0 0",
        fontBase: "24 24 26",
      },
      undefined,
      personality,
    ),
  ] as const;
}

export const markMonochromeDarkTheme = buildDarkTheme(
  "mark-monochrome-dark",
  darkSurfacesMedousaBrand,
  {
    primary: "242 239 230",
    secondary: "122 122 136",
    onPrimary: "0 0 0",
    onSurface: "242 239 230",
    fontBase: "226 223 216",
  },
  undefined,
  {
    roles: { focus: "194 192 188", decorative: "122 122 136" },
    effects: {
      glow: "242 239 230",
      gradientA: "242 239 230",
      gradientB: "160 159 158",
      gradientC: "85 85 95",
      glowStrength: "0.04",
    },
    shape: { controlRadius: "0.25rem", containerRadius: "0.4rem" },
  },
);

export const markMonochromeLightTheme = buildTheme(
  "mark-monochrome-light",
  lightSurfacesMedousaBrand,
  {
    primary: "0 0 0",
    secondary: "122 122 136",
    onPrimary: "242 239 230",
    onSurface: "0 0 0",
    fontBase: "24 24 26",
  },
  undefined,
  {
    roles: { focus: "85 85 95", decorative: "122 122 136" },
    effects: { glowStrength: "0.03" },
    shape: { controlRadius: "0.25rem", containerRadius: "0.4rem" },
  },
);

export const markInkBlackDarkTheme = buildDarkTheme(
  "mark-ink-black-dark",
  tintSurfaceScale(darkSurfacesMedousaBrand, "92 72 46", "dark", 0.55),
  {
    primary: "242 239 230",
    secondary: "85 85 95",
    onPrimary: "0 0 0",
    onSurface: "242 239 230",
    fontBase: "226 223 216",
  },
  undefined,
  {
    roles: { focus: "242 239 230", decorative: "185 109 16" },
    effects: {
      glow: "185 109 16",
      gradientA: "242 239 230",
      gradientB: "194 192 188",
      gradientC: "185 109 16",
      glowStrength: "0.08",
    },
    shape: { controlRadius: "0.75rem", containerRadius: "0.9rem" },
  },
);

export const markInkBlackLightTheme = buildTheme(
  "mark-ink-black-light",
  tintSurfaceScale(lightSurfacesMedousaBrand, "185 109 16", "light", 0.38),
  {
    primary: "0 0 0",
    secondary: "85 85 95",
    onPrimary: "242 239 230",
    onSurface: "0 0 0",
    fontBase: "24 24 26",
  },
  undefined,
  {
    roles: { focus: "85 85 95", decorative: "185 109 16" },
    effects: { glow: "185 109 16", glowStrength: "0.06" },
    shape: { controlRadius: "0.75rem", containerRadius: "0.9rem" },
  },
);

export const [markNebulaPurpleDarkTheme, markNebulaPurpleLightTheme] = markPair(
  "mark-nebula-purple",
  {
    primary: "168 85 247",
    secondary: "244 114 182",
    tertiary: "124 58 237",
    surfaceTint: "88 28 135",
    darkOnPrimary: "0 0 0",
    lightOnPrimary: "0 0 0",
    personality: { effects: { glowStrength: "0.21" } },
  },
);

export const [markOceanBlueDarkTheme, markOceanBlueLightTheme] = markPair(
  "mark-ocean-blue",
  {
    primary: "56 189 248",
    secondary: "29 78 216",
    tertiary: "34 211 238",
    surfaceTint: "8 47 73",
    darkOnPrimary: "0 0 0",
    lightOnPrimary: "0 0 0",
  },
);

export const [markAbyssTealDarkTheme, markAbyssTealLightTheme] = markPair(
  "mark-abyss-teal",
  {
    primary: "45 212 191",
    secondary: "56 189 248",
    tertiary: "15 155 124",
    surfaceTint: "4 47 46",
    darkOnPrimary: "0 0 0",
    lightOnPrimary: "0 0 0",
  },
);

export const [markAmberGoldDarkTheme, markAmberGoldLightTheme] = markPair(
  "mark-amber-gold",
  {
    primary: "245 184 65",
    secondary: "185 109 16",
    tertiary: "251 146 60",
    surfaceTint: "69 26 3",
    darkOnPrimary: "0 0 0",
    lightOnPrimary: "0 0 0",
  },
);

export const [markVioletDarkTheme, markVioletLightTheme] = markPair("mark-violet", {
  primary: "124 58 237",
  secondary: "56 189 248",
  tertiary: "168 85 247",
  surfaceTint: "46 16 101",
  surfaceStrength: 0.82,
  personality: {
    shape: { controlRadius: "0.5rem", containerRadius: "0.65rem" },
  },
});

export const [markDeepBlueDarkTheme, markDeepBlueLightTheme] = markPair(
  "mark-deep-blue",
  {
    primary: "29 78 216",
    secondary: "99 102 241",
    tertiary: "56 189 248",
    surfaceTint: "23 37 84",
    surfaceStrength: 1.15,
    personality: { effects: { glowStrength: "0.12" } },
  },
);

export const [markJadeDarkTheme, markJadeLightTheme] = markPair("mark-jade", {
  primary: "15 155 124",
  secondary: "163 190 140",
  tertiary: "45 212 191",
  surfaceTint: "5 46 22",
  surfaceStrength: 1.08,
  darkOnPrimary: "0 0 0",
  lightOnPrimary: "0 0 0",
  personality: { effects: { glowStrength: "0.14" } },
});

export const [markAuroraDarkTheme, markAuroraLightTheme] = markPair("mark-aurora", {
  primary: "244 114 182",
  secondary: "168 85 247",
  tertiary: "56 189 248",
  surfaceTint: "76 29 149",
  surfaceStrength: 0.9,
  darkOnPrimary: "0 0 0",
  lightOnPrimary: "0 0 0",
  personality: {
    roles: { decorative: "52 211 153" },
    charts: { four: "52 211 153", five: "245 184 65" },
    effects: {
      gradientA: "244 114 182",
      gradientB: "168 85 247",
      gradientC: "56 189 248",
      glowStrength: "0.24",
    },
  },
});

export const brandMarkThemes = [
  markMonochromeDarkTheme,
  markMonochromeLightTheme,
  markInkBlackDarkTheme,
  markInkBlackLightTheme,
  markNebulaPurpleDarkTheme,
  markNebulaPurpleLightTheme,
  markOceanBlueDarkTheme,
  markOceanBlueLightTheme,
  markAbyssTealDarkTheme,
  markAbyssTealLightTheme,
  markAmberGoldDarkTheme,
  markAmberGoldLightTheme,
  markVioletDarkTheme,
  markVioletLightTheme,
  markDeepBlueDarkTheme,
  markDeepBlueLightTheme,
  markJadeDarkTheme,
  markJadeLightTheme,
  markAuroraDarkTheme,
  markAuroraLightTheme,
];
