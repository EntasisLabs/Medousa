export type SettingsSectionId =
  | "preferences"
  | "agent"
  | "reach"
  | "shell"
  | "versions"
  | "engine"
  | "shared"
  | "phone"
  | "nearby"
  | "channels"
  | "packages"
  | "basement";

/** Quiet TOC chapters — not separate product surfaces. */
export type SettingsSectionGroupId =
  | "space"
  | "her"
  | "tools"
  | "people"
  | "machine";

export const SETTINGS_SECTION_GROUPS: {
  id: SettingsSectionGroupId;
  label: string;
}[] = [
  { id: "space", label: "Space" },
  { id: "her", label: "Her" },
  { id: "tools", label: "Tools" },
  { id: "people", label: "People" },
  { id: "machine", label: "Machine" },
];

export type SettingsSectionDef = {
  id: SettingsSectionId;
  label: string;
  hint: string;
  group: SettingsSectionGroupId;
};

export const SETTINGS_SECTIONS: SettingsSectionDef[] = [
  {
    id: "preferences",
    label: "Preferences",
    hint: "Look, notifications & chrome",
    group: "space",
  },
  {
    id: "agent",
    label: "Medousa Agent",
    hint: "Stance, memory & models",
    group: "her",
  },
  { id: "reach", label: "Reach", hint: "Tools she may use", group: "tools" },
  { id: "shell", label: "Shell", hint: "Process sandbox & commands", group: "tools" },
  { id: "engine", label: "Engine", hint: "Speed, budgets & diagnostics", group: "tools" },
  { id: "shared", label: "Shared", hint: "Team seats on this brain", group: "people" },
  { id: "phone", label: "Phone", hint: "Pair your phone", group: "people" },
  { id: "nearby", label: "Nearby", hint: "Peers, bundles & trust", group: "people" },
  { id: "channels", label: "Channels", hint: "Telegram, Discord, Slack & more", group: "people" },
  { id: "versions", label: "Versions", hint: "Vault history (optional)", group: "machine" },
  { id: "packages", label: "Packages", hint: "Offline brain, adapters & MCP", group: "machine" },
  {
    id: "basement",
    label: "Connection",
    hint: "This device, engine & advanced files",
    group: "machine",
  },
];

export function settingsSectionById(id: SettingsSectionId): SettingsSectionDef | undefined {
  return SETTINGS_SECTIONS.find((section) => section.id === id);
}

export function settingsGroupLabel(groupId: SettingsSectionGroupId): string {
  return SETTINGS_SECTION_GROUPS.find((group) => group.id === groupId)?.label ?? groupId;
}

/** Sections in TOC order, with a group header whenever the chapter changes. */
export function settingsNavEntries(): Array<
  | { kind: "group"; id: SettingsSectionGroupId; label: string }
  | { kind: "section"; section: SettingsSectionDef }
> {
  const entries: Array<
    | { kind: "group"; id: SettingsSectionGroupId; label: string }
    | { kind: "section"; section: SettingsSectionDef }
  > = [];
  let lastGroup: SettingsSectionGroupId | null = null;
  for (const section of SETTINGS_SECTIONS) {
    if (section.group !== lastGroup) {
      entries.push({
        kind: "group",
        id: section.group,
        label: settingsGroupLabel(section.group),
      });
      lastGroup = section.group;
    }
    entries.push({ kind: "section", section });
  }
  return entries;
}

export const DEPTH_CHARTER_OPTIONS = [
  {
    id: "concise" as const,
    label: "Concise",
    hint: "Short answers — less reasoning on the page",
  },
  {
    id: "standard" as const,
    label: "Standard",
    hint: "Balanced depth for everyday work",
  },
  {
    id: "deep" as const,
    label: "Deep",
    hint: "More thorough reasoning and detail",
  },
];

export const TOOL_CALL_CHARTER_OPTIONS = [
  {
    id: "auto" as const,
    label: "Flexible",
    hint: "She decides when tools are worth calling",
  },
  {
    id: "strict" as const,
    label: "Careful",
    hint: "Tighter rules before invoking tools",
  },
] as const;

export const HOST_BUS_CHARTER_OPTIONS = [
  {
    id: "auto" as const,
    label: "When needed",
    hint: "Bring specialists in only when the turn needs help",
  },
  {
    id: "force" as const,
    label: "Always",
    hint: "Route through the specialist bus every turn",
  },
  {
    id: "off" as const,
    label: "Direct",
    hint: "Orchestrator only — no specialist bus",
  },
] as const;
