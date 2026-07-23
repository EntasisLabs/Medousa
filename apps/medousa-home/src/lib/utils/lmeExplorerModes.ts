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

export const LME_EXPLORER_MODES: LmeExplorerModeDef[] = [
  { id: "notes", label: "Notes", icon: BookOpen },
  { id: "files", label: "Local Files", icon: Files },
  { id: "presentations", label: "Presentations", icon: Presentation },
  { id: "scripts", label: "Scripts", icon: FileCode2 },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "flows", label: "Flows", icon: GitBranch },
  { id: "schedules", label: "Schedules", icon: CalendarClock },
  { id: "history", label: "History", icon: History },
];

const MODE_IDS = new Set<string>(LME_EXPLORER_MODES.map((mode) => mode.id));

export function isLmeExplorerMode(value: string): value is LmeExplorerMode {
  return MODE_IDS.has(value);
}

export function labelForLmeExplorerMode(mode: LmeExplorerMode): string {
  return LME_EXPLORER_MODES.find((entry) => entry.id === mode)?.label ?? "Workspace";
}
