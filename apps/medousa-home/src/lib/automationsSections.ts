import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
import { Bot, CalendarClock, FileCode2, GitBranch, History } from "@lucide/svelte";

export const AUTOMATIONS_SECTIONS = [
  { id: "scripts", label: "Scripts", icon: FileCode2 },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "flows", label: "Flows", icon: GitBranch },
  { id: "schedules", label: "Schedules", icon: CalendarClock },
  { id: "history", label: "History", icon: History },
] satisfies { id: AutomationsSection; label: string; icon: typeof FileCode2 }[];
