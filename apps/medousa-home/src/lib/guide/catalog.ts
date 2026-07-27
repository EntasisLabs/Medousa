import type { GuideChapter, GuideGroup } from "./types";

export const GUIDE_GROUPS: GuideGroup[] = [
  { id: "start", label: "Start" },
  { id: "everyday", label: "Everyday" },
  { id: "create", label: "Create" },
  { id: "connect", label: "Connect" },
  { id: "more", label: "More" },
];

export const GUIDE_CHAPTERS: GuideChapter[] = [
  {
    id: "welcome",
    title: "Welcome",
    file: "00-welcome.md",
    group: "start",
    summary: "What Medousa is and how to use this guide",
  },
  {
    id: "find-answers",
    title: "Find answers",
    file: "01b-find-answers.md",
    group: "start",
    summary: "Short answers to common questions",
  },
  {
    id: "getting-started",
    title: "Getting started",
    file: "02-getting-started.md",
    group: "start",
    summary: "First run, connection, and your first hour",
  },
  {
    id: "architecture",
    title: "How Medousa fits together",
    file: "01-architecture-terminology.md",
    group: "start",
    summary: "Workshop, phone, peers, and plain glossary",
  },
  {
    id: "navigation-surfaces",
    title: "Navigation and surfaces",
    file: "03-navigation-surfaces.md",
    group: "everyday",
    summary: "Where things live on desktop and phone",
  },
  {
    id: "chat",
    title: "Chat",
    file: "04-chat.md",
    group: "everyday",
    summary: "Send messages, pick models, manage conversations",
  },
  {
    id: "work-jobs",
    title: "Work and background jobs",
    file: "13-work-background-jobs.md",
    group: "everyday",
    summary: "The Work board and background asks",
  },
  {
    id: "themes-customization",
    title: "Themes and look",
    file: "05-themes-customization.md",
    group: "everyday",
    summary: "Colors, zoom, and quieter screens",
  },
  {
    id: "calendar",
    title: "Calendar",
    file: "17-calendar.md",
    group: "everyday",
    summary: "Day, week, month, and calendar files",
  },
  {
    id: "profiles-locus",
    title: "You and the Map",
    file: "19-profiles-locus.md",
    group: "everyday",
    summary: "Teach Medousa about you; browse recent links",
  },
  {
    id: "troubleshooting",
    title: "Troubleshooting",
    file: "14-troubleshooting.md",
    group: "everyday",
    summary: "What to try when something fails",
  },
  {
    id: "vault-notes",
    title: "Vault and notes",
    file: "06-vault-notes.md",
    group: "create",
    summary: "Library, editing, links, and export",
  },
  {
    id: "vault-recovery",
    title: "Trash and versions",
    file: "15-vault-recovery.md",
    group: "create",
    summary: "Restore deleted notes and optional history",
  },
  {
    id: "views-environments",
    title: "Views and layouts",
    file: "07-views-environments.md",
    group: "create",
    summary: "Custom screens, widgets, and backups",
  },
  {
    id: "grapheme-automations",
    title: "Automations and scripts",
    file: "08-grapheme-automations.md",
    group: "create",
    summary: "Scripts, flows, and schedules",
  },
  {
    id: "writing-scripts",
    title: "Writing scripts",
    file: "08b-writing-scripts.md",
    group: "create",
    summary: "Grapheme language, pipes, and starter examples",
  },
  {
    id: "specialist-agents",
    title: "Agents",
    file: "18-specialist-agents.md",
    group: "create",
    summary: "Import and run specialist skills",
  },
  {
    id: "liquid-reference",
    title: "Liquid blocks",
    file: "23-liquid-reference.md",
    group: "create",
    summary: "Interactive blocks in notes (advanced)",
  },
  {
    id: "sharing-phone",
    title: "Sharing and phone",
    file: "11-sharing-phone.md",
    group: "connect",
    summary: "Phone pairing, peers, and shared seats",
  },
  {
    id: "messaging-channels",
    title: "Messaging channels",
    file: "20-messaging-channels.md",
    group: "connect",
    summary: "Telegram, Discord, Slack, WhatsApp",
  },
  {
    id: "workshops-connections",
    title: "Workshops and connections",
    file: "09-workshops-connections.md",
    group: "connect",
    summary: "Switch workshops, engine health, updates",
  },
  {
    id: "permissions-budgets",
    title: "Permissions and approvals",
    file: "12-permissions-budgets.md",
    group: "more",
    summary: "Allow/Deny, tool rounds, and tool safety (advanced)",
  },
  {
    id: "browser",
    title: "Browser and web research",
    file: "16-browser-web.md",
    group: "more",
    summary: "Built-in browser and when Medousa needs your help",
  },
  {
    id: "runtime-telemetry",
    title: "Runtime status",
    file: "21-runtime-telemetry.md",
    group: "more",
    summary: "What’s running, jobs, and delivery (advanced)",
  },
  {
    id: "mcp-packages",
    title: "Packages and MCP",
    file: "22-mcp-packages.md",
    group: "more",
    summary: "Optional installs and external tools (advanced)",
  },
  {
    id: "keyboard-flow",
    title: "Keyboard and flow",
    file: "10-keyboard-flow.md",
    group: "more",
    summary: "Everyday shortcuts and Spotlight habits",
  },
  {
    id: "commands-reference",
    title: "Commands and keyboard reference",
    file: "24-commands-reference.md",
    group: "more",
    summary: "Full shortcut and Spotlight list",
  },
  {
    id: "settings-reference",
    title: "Settings reference",
    file: "25-settings-reference.md",
    group: "more",
    summary: "What each Settings section controls (advanced)",
  },
  {
    id: "platform-matrix",
    title: "Desktop, web, and phone",
    file: "26-platform-matrix.md",
    group: "more",
    summary: "What each kind of app can do",
  },
  {
    id: "data-lifecycle",
    title: "Where your data lives",
    file: "27-data-lifecycle.md",
    group: "more",
    summary: "Folders, backups, and cleanup (advanced)",
  },
  {
    id: "operator-recipes",
    title: "How-to recipes",
    file: "28-operator-recipes.md",
    group: "more",
    summary: "Step-by-step recipes for common jobs",
  },
  {
    id: "faq-limits",
    title: "FAQ and limits",
    file: "29-faq-limits.md",
    group: "more",
    summary: "Short answers and hard limits",
  },
  {
    id: "whats-new",
    title: "What’s new",
    file: "30-whats-new.md",
    group: "more",
    summary: "Recent product changes and compatibility",
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
