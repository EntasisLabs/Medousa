import type { GuideChapter, GuideGroup } from "./types";

export const GUIDE_GROUPS: GuideGroup[] = [
  { id: "start", label: "Start" },
  { id: "workspace", label: "Workspace" },
  { id: "craft", label: "Craft" },
  { id: "system", label: "System" },
];

export const GUIDE_CHAPTERS: GuideChapter[] = [
  {
    id: "welcome",
    title: "Welcome",
    file: "00-welcome.md",
    group: "start",
    summary: "What Medousa is and how this guide works",
  },
  {
    id: "getting-started",
    title: "Getting started",
    file: "01-getting-started.md",
    group: "start",
    summary: "Workshops, connection, and the first loop",
  },
  {
    id: "navigation-surfaces",
    title: "Navigation and surfaces",
    file: "02-navigation-surfaces.md",
    group: "workspace",
    summary: "Rails, surfaces, panes, desktops, and pop-outs",
  },
  {
    id: "chat",
    title: "Chat",
    file: "03-chat.md",
    group: "workspace",
    summary: "Composer, models, runtime, and turn controls",
  },
  {
    id: "themes-customization",
    title: "Themes and customization",
    file: "04-themes-customization.md",
    group: "workspace",
    summary: "Themes, zoom, and quiet chrome",
  },
  {
    id: "vault-notes",
    title: "Vault and notes",
    file: "05-vault-notes.md",
    group: "craft",
    summary: "Vault browser, notes, and sticky windows",
  },
  {
    id: "views-environments",
    title: "Views and environments",
    file: "06-views-environments.md",
    group: "craft",
    summary: "Custom views, environments, and layouts",
  },
  {
    id: "grapheme-automations",
    title: "Grapheme and automations",
    file: "07-grapheme-automations.md",
    group: "craft",
    summary: "Scripts, recipes, flows, and host modules",
  },
  {
    id: "workshops-connections",
    title: "Workshops and connections",
    file: "08-workshops-connections.md",
    group: "system",
    summary: "Active workshop, pairing, engine, and updates",
  },
  {
    id: "keyboard-flow",
    title: "Keyboard and flow",
    file: "09-keyboard-flow.md",
    group: "system",
    summary: "Shortcuts, panes, cheat sheet, desktop toolbar",
  },
  {
    id: "sharing-phone",
    title: "Sharing and phone",
    file: "10-sharing-phone.md",
    group: "system",
    summary: "LAN reachability and phone companions",
  },
];

export const DEFAULT_GUIDE_CHAPTER_ID = "welcome";

export function getGuideChapter(id: string | null | undefined): GuideChapter | null {
  const needle = id?.trim();
  if (!needle) return null;
  return GUIDE_CHAPTERS.find((chapter) => chapter.id === needle) ?? null;
}

export function chaptersByGroup(): { group: GuideGroup; chapters: GuideChapter[] }[] {
  return GUIDE_GROUPS.map((group) => ({
    group,
    chapters: GUIDE_CHAPTERS.filter((chapter) => chapter.group === group.id),
  })).filter((entry) => entry.chapters.length > 0);
}
