import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
import { CalendarClock, FileCode2, GitBranch } from "@lucide/svelte";

export const AUTOMATIONS_SECTIONS = [
  { id: "scripts", label: "Scripts", icon: FileCode2 },
  { id: "flows", label: "Flows", icon: GitBranch },
  { id: "schedules", label: "Schedules", icon: CalendarClock },
] satisfies { id: AutomationsSection; label: string; icon: typeof FileCode2 }[];
