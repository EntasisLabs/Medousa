import type { ColorThemeId } from "$lib/theme/themeRegistry";

export type MedousaMarkId =
  | "monochrome"
  | "ink-black"
  | "nebula-purple"
  | "ocean-blue"
  | "abyss-teal"
  | "amber-gold"
  | "violet"
  | "deep-blue"
  | "jade"
  | "aurora";

export interface MedousaMarkOption {
  id: MedousaMarkId;
  label: string;
  tagline: string;
  darkColor: string;
  lightColor: string;
  previewBackground: string;
  pairedThemeId: ColorThemeId;
}

export const MEDOUSA_MARK_OPTIONS: MedousaMarkOption[] = [
  {
    id: "monochrome",
    label: "Monochrome",
    tagline: "Timeless · clean · iconic",
    darkColor: "#F2EFE6",
    lightColor: "#000000",
    previewBackground: "#000000",
    pairedThemeId: "mark-monochrome",
  },
  {
    id: "ink-black",
    label: "Ink Black",
    tagline: "Bold · minimal · strong",
    darkColor: "#000000",
    lightColor: "#000000",
    previewBackground: "#F2EFE6",
    pairedThemeId: "mark-ink-black",
  },
  {
    id: "nebula-purple",
    label: "Nebula Purple",
    tagline: "Intelligent · mysterious · deep",
    darkColor: "#A855F7",
    lightColor: "#A855F7",
    previewBackground: "#000000",
    pairedThemeId: "mark-nebula-purple",
  },
  {
    id: "ocean-blue",
    label: "Ocean Blue",
    tagline: "Calm · focused · infinite",
    darkColor: "#38BDF8",
    lightColor: "#38BDF8",
    previewBackground: "#000000",
    pairedThemeId: "mark-ocean-blue",
  },
  {
    id: "abyss-teal",
    label: "Abyss Teal",
    tagline: "Organic · balanced · flowing",
    darkColor: "#2DD4BF",
    lightColor: "#2DD4BF",
    previewBackground: "#000000",
    pairedThemeId: "mark-abyss-teal",
  },
  {
    id: "amber-gold",
    label: "Amber Gold",
    tagline: "Energetic · warm · inventive",
    darkColor: "#F5B841",
    lightColor: "#F5B841",
    previewBackground: "#000000",
    pairedThemeId: "mark-amber-gold",
  },
  {
    id: "violet",
    label: "Violet",
    tagline: "Creative · intelligent · modern",
    darkColor: "#7C3AED",
    lightColor: "#7C3AED",
    previewBackground: "#000000",
    pairedThemeId: "mark-violet",
  },
  {
    id: "deep-blue",
    label: "Deep Blue",
    tagline: "Trust · depth · stability",
    darkColor: "#1D4ED8",
    lightColor: "#1D4ED8",
    previewBackground: "#000000",
    pairedThemeId: "mark-deep-blue",
  },
  {
    id: "jade",
    label: "Jade",
    tagline: "Natural · harmonic · clear",
    darkColor: "#0F9B7C",
    lightColor: "#0F9B7C",
    previewBackground: "#000000",
    pairedThemeId: "mark-jade",
  },
  {
    id: "aurora",
    label: "Aurora",
    tagline: "Dynamic · ethereal · limitless",
    darkColor: "#A855F7",
    lightColor: "#A855F7",
    previewBackground: "#000000",
    pairedThemeId: "mark-aurora",
  },
];

export const DEFAULT_MEDOUSA_MARK: MedousaMarkId = "violet";

export function isMedousaMarkId(value: string | null | undefined): value is MedousaMarkId {
  return MEDOUSA_MARK_OPTIONS.some((option) => option.id === value);
}

export function medousaMarkOption(id: MedousaMarkId): MedousaMarkOption {
  return (
    MEDOUSA_MARK_OPTIONS.find((option) => option.id === id) ??
    MEDOUSA_MARK_OPTIONS.find((option) => option.id === DEFAULT_MEDOUSA_MARK)!
  );
}

export function markForTheme(themeId: ColorThemeId): MedousaMarkId {
  const exact = MEDOUSA_MARK_OPTIONS.find((option) => option.pairedThemeId === themeId);
  if (exact) return exact.id;
  switch (themeId) {
    case "black-lily":
      return "aurora";
    case "caduceus":
      return "jade";
    case "ember":
    case "hearth":
      return "amber-gold";
    case "cupertino":
    case "midnight":
    case "github":
    case "nord":
    case "solarized":
      return "ocean-blue";
    case "graphite":
      return "monochrome";
    case "dracula":
    case "catppuccin":
      return "nebula-purple";
    default:
      return "violet";
  }
}
