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
    id: "architecture",
    title: "Architecture and terminology",
    file: "01-architecture-terminology.md",
    group: "start",
    summary: "Home, workshop, peers, profiles, and state scopes",
  },
  {
    id: "getting-started",
    title: "Getting started",
    file: "02-getting-started.md",
    group: "start",
    summary: "Wizard, first connection, and the daily loop",
  },
  {
    id: "navigation-surfaces",
    title: "Navigation and surfaces",
    file: "03-navigation-surfaces.md",
    group: "workspace",
    summary: "Surface inventory, Library/Automations modes, panes, mobile",
  },
  {
    id: "chat",
    title: "Chat",
    file: "04-chat.md",
    group: "workspace",
    summary: "Composer, models, sessions, slash commands, and turn controls",
  },
  {
    id: "permissions-budgets",
    title: "Permissions, budgets, and tool safety",
    file: "12-permissions-budgets.md",
    group: "workspace",
    summary: "Allow/Deny, tool rounds, Runtime Controls, browser verification",
  },
  {
    id: "work-jobs",
    title: "Work and background jobs",
    file: "13-work-background-jobs.md",
    group: "workspace",
    summary: "Work board, /ask, cancellation, retention, Runtime vs Work",
  },
  {
    id: "browser",
    title: "Browser and web research",
    file: "16-browser-web.md",
    group: "workspace",
    summary: "Web tabs, bookmarks, agent handoff, and verification",
  },
  {
    id: "calendar",
    title: "Calendar",
    file: "17-calendar.md",
    group: "workspace",
    summary: "Day/week/month events and .ics import/export",
  },
  {
    id: "profiles-locus",
    title: "Profiles, identity, and Locus",
    file: "19-profiles-locus.md",
    group: "workspace",
    summary: "You / teach / people, and the session link map",
  },
  {
    id: "themes-customization",
    title: "Themes, accessibility, and quiet chrome",
    file: "05-themes-customization.md",
    group: "workspace",
    summary: "Light/dark, named themes, zoom, reduced motion, Everyday prefs",
  },
  {
    id: "vault-notes",
    title: "Vault and notes",
    file: "06-vault-notes.md",
    group: "craft",
    summary: "Library, Live/Build/Preview, links, export, conflicts",
  },
  {
    id: "vault-recovery",
    title: "Vault trash and versions",
    file: "15-vault-recovery.md",
    group: "craft",
    summary: "Trash restore and optional Git versions",
  },
  {
    id: "liquid-reference",
    title: "Liquid reference",
    file: "23-liquid-reference.md",
    group: "craft",
    summary: "Fence vocabulary, charts, feeds, and examples",
  },
  {
    id: "views-environments",
    title: "Views and environments",
    file: "07-views-environments.md",
    group: "craft",
    summary: "Custom views, widgets, tiling, feeds, backup/import",
  },
  {
    id: "grapheme-automations",
    title: "Grapheme and automations",
    file: "08-grapheme-automations.md",
    group: "craft",
    summary: "Scripts, templates, flows, schedules, and history",
  },
  {
    id: "specialist-agents",
    title: "Specialist agents",
    file: "18-specialist-agents.md",
    group: "craft",
    summary: "Import SKILL.md agents, tools, run, and schedule",
  },
  {
    id: "workshops-connections",
    title: "Workshops and connections",
    file: "09-workshops-connections.md",
    group: "system",
    summary: "Active workshop, pairing, engine, and updates",
  },
  {
    id: "keyboard-flow",
    title: "Keyboard and flow",
    file: "10-keyboard-flow.md",
    group: "system",
    summary: "Essentials, Spotlight habits, panes — full tables in Commands reference",
  },
  {
    id: "commands-reference",
    title: "Commands and keyboard reference",
    file: "24-commands-reference.md",
    group: "system",
    summary: "Generated shortcuts, slash commands, and Spotlight inventory",
  },
  {
    id: "settings-reference",
    title: "Settings reference",
    file: "25-settings-reference.md",
    group: "system",
    summary: "Sections by scope, platform, restart, and risk",
  },
  {
    id: "platform-matrix",
    title: "Desktop, web, and phone",
    file: "26-platform-matrix.md",
    group: "system",
    summary: "Capability matrix across shells",
  },
  {
    id: "data-lifecycle",
    title: "Data locations, backup, and retention",
    file: "27-data-lifecycle.md",
    group: "system",
    summary: "Paths, backups, work/presentation retention, migration",
  },
  {
    id: "sharing-phone",
    title: "Sharing and phone",
    file: "11-sharing-phone.md",
    group: "system",
    summary: "Phone pairing, peers, Shared mode, and LAN trust",
  },
  {
    id: "messaging-channels",
    title: "Messaging channels",
    file: "20-messaging-channels.md",
    group: "system",
    summary: "Telegram, Discord, Slack, WhatsApp setup and allowlists",
  },
  {
    id: "runtime-telemetry",
    title: "Runtime telemetry",
    file: "21-runtime-telemetry.md",
    group: "system",
    summary: "Now, Jobs, Delivery, and Routing diagnostics",
  },
  {
    id: "mcp-packages",
    title: "MCP and packages",
    file: "22-mcp-packages.md",
    group: "system",
    summary: "Optional binaries, MCP gateway, and external servers",
  },
  {
    id: "operator-recipes",
    title: "Operator recipes",
    file: "28-operator-recipes.md",
    group: "system",
    summary: "Runbooks for research, schedules, peers, recovery, and more",
  },
  {
    id: "faq-limits",
    title: "Known limits and FAQ",
    file: "29-faq-limits.md",
    group: "system",
    summary: "Hard caps and short answers",
  },
  {
    id: "whats-new",
    title: "What’s new and compatibility",
    file: "30-whats-new.md",
    group: "system",
    summary: "Guide milestones, renamed labels, build checks",
  },
  {
    id: "docs-governance",
    title: "Documentation governance",
    file: "31-docs-governance.md",
    group: "system",
    summary: "Feature→doc checklist and release gates for this guide",
  },
  {
    id: "troubleshooting",
    title: "Troubleshooting",
    file: "14-troubleshooting.md",
    group: "system",
    summary: "Symptom → first checks for connection, chat, tools, and pairing",
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
