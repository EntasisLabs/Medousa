import { thisHostLabel } from "$lib/platformCopy";

export type SettingsSectionId =
  | "preferences"
  | "agent"
  | "runtime"
  | "network"
  | "packages"
  | "mcp"
  | "basement";

/** Quiet TOC groups — unlabeled app block, then this host. */
export type SettingsSectionGroupId = "app" | "machine";

/** Mobile settings pager order — every section, one at a time with arrows. */
export const SETTINGS_MOBILE_SECTIONS: SettingsSectionId[] = [
  "preferences",
  "agent",
  "runtime",
  "network",
  "packages",
  "mcp",
  "basement",
];

export const SETTINGS_SECTION_GROUPS: {
  id: SettingsSectionGroupId;
  /** Empty = no header in the rail (top app block). Machine label is dynamic. */
  label: string;
}[] = [
  { id: "app", label: "" },
  { id: "machine", label: "" },
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
    group: "app",
  },
  {
    id: "agent",
    label: "Medousa Agent",
    hint: "Stance, memory & models",
    group: "app",
  },
  {
    id: "runtime",
    label: "Runtime Controls",
    hint: "Reach, shell, engine & versions",
    group: "app",
  },
  {
    id: "network",
    label: "Sharing",
    hint: "Seats, phone, peers & channels",
    group: "app",
  },
  {
    id: "packages",
    label: "Packages",
    hint: "Channel adapters & installs",
    group: "machine",
  },
  {
    id: "mcp",
    label: "MCP",
    hint: "Gateway & tool servers",
    group: "machine",
  },
  {
    id: "basement",
    label: "Workshop",
    hint: "Active workshop, engine & files",
    group: "machine",
  },
];

export function settingsSectionById(id: SettingsSectionId): SettingsSectionDef | undefined {
  return SETTINGS_SECTIONS.find((section) => section.id === id);
}

export function settingsGroupLabel(groupId: SettingsSectionGroupId): string {
  if (groupId === "machine") return thisHostLabel();
  return SETTINGS_SECTION_GROUPS.find((group) => group.id === groupId)?.label ?? groupId;
}

/** Sections in TOC order; group headers only when the group has a label. */
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
      const label = settingsGroupLabel(section.group);
      if (label) {
        entries.push({
          kind: "group",
          id: section.group,
          label,
        });
      }
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
