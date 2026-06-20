import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";

export const AUTOMATIONS_SECTIONS: { id: AutomationsSection; label: string }[] = [
  { id: "scripts", label: "Scripts" },
  { id: "flows", label: "Flows" },
  { id: "schedules", label: "Schedules" },
  { id: "history", label: "History" },
];
