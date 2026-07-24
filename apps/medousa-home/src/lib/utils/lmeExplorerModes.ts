import {
  BookOpen,
  Bot,
  CalendarClock,
  FileCode2,
  Files,
  GitBranch,
  History,
  Presentation,
} from "@lucide/svelte";
import type { LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";

export type LmeExplorerModeDef = {
  id: LmeExplorerMode;
  label: string;
  icon: typeof BookOpen;
};

export type LmeExplorerFamily = "library" | "automations";

export const LME_LIBRARY_MODES: LmeExplorerModeDef[] = [
  { id: "notes", label: "Notes", icon: BookOpen },
  { id: "files", label: "Local Files", icon: Files },
  { id: "presentations", label: "Presentations", icon: Presentation },
];

/** Page 0 of the Automations strip. */
export const LME_AUTOMATIONS_PRIMARY_MODES: LmeExplorerModeDef[] = [
  { id: "scripts", label: "Scripts", icon: FileCode2 },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "flows", label: "Flows", icon: GitBranch },
  { id: "schedules", label: "Schedules", icon: CalendarClock },
];

/** Page 1 of the Automations strip (revealed via chevron). */
export const LME_AUTOMATIONS_SECONDARY_MODES: LmeExplorerModeDef[] = [
  { id: "history", label: "History", icon: History },
];

export const LME_AUTOMATIONS_MODES: LmeExplorerModeDef[] = [
  ...LME_AUTOMATIONS_PRIMARY_MODES,
  ...LME_AUTOMATIONS_SECONDARY_MODES,
];

export const LME_EXPLORER_MODES: LmeExplorerModeDef[] = [
  ...LME_LIBRARY_MODES,
  ...LME_AUTOMATIONS_MODES,
];

const MODE_IDS = new Set<string>(LME_EXPLORER_MODES.map((mode) => mode.id));
const LIBRARY_IDS = new Set<LmeExplorerMode>(LME_LIBRARY_MODES.map((mode) => mode.id));
const AUTOMATIONS_IDS = new Set<LmeExplorerMode>(
  LME_AUTOMATIONS_MODES.map((mode) => mode.id),
);
const AUTOMATIONS_SECONDARY_IDS = new Set<LmeExplorerMode>(
  LME_AUTOMATIONS_SECONDARY_MODES.map((mode) => mode.id),
);

export function isLmeExplorerMode(value: string): value is LmeExplorerMode {
  return MODE_IDS.has(value);
}

export function isLmeLibraryMode(mode: LmeExplorerMode): boolean {
  return LIBRARY_IDS.has(mode);
}

export function isLmeAutomationsMode(mode: LmeExplorerMode): boolean {
  return AUTOMATIONS_IDS.has(mode);
}

export function familyForLmeExplorerMode(mode: LmeExplorerMode): LmeExplorerFamily {
  return isLmeAutomationsMode(mode) ? "automations" : "library";
}

/** Map an open LME tab kind → Library vs Automations (for summon / focus hints). */
export function familyForLmeTabKind(kind: string): LmeExplorerFamily | null {
  switch (kind) {
    case "script":
    case "manuscript":
    case "flow":
    case "schedule":
      return "automations";
    case "note":
    case "file":
    case "deck":
      return "library";
    default:
      return null;
  }
}

/** Automations strip page: 0 = scripts/agents/flows/schedules, 1 = history. */
export function automationsStripPageForMode(mode: LmeExplorerMode): 0 | 1 {
  return AUTOMATIONS_SECONDARY_IDS.has(mode) ? 1 : 0;
}

export function defaultModeForLmeFamily(family: LmeExplorerFamily): LmeExplorerMode {
  return family === "automations" ? "scripts" : "notes";
}

export function labelForLmeExplorerMode(mode: LmeExplorerMode): string {
  return LME_EXPLORER_MODES.find((entry) => entry.id === mode)?.label ?? "Workspace";
}
