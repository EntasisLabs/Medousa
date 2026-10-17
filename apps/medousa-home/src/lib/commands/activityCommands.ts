import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import type { WorkshopCommand } from "./types";

export function buildActivityCommands(): WorkshopCommand[] {
  return [{
    id: "workshop-activity",
    section: "go",
    label: "Workshop activity",
    subtitle: "Tasks, agents, scripts, and recent activity",
    keywords: "work tasks jobs blocked activity",
    run: () => commandSpotlight.openActivity(),
  }];
}
