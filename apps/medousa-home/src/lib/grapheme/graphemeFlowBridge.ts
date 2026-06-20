import { automationsNav } from "$lib/stores/automationsNav.svelte";
import { flows } from "$lib/stores/flows.svelte";
import { layout } from "$lib/stores/layout.svelte";

export function promoteScriptToFlow(source: string, scriptName?: string | null): void {
  const trimmed = source.trim();
  if (!trimmed) {
    throw new Error("Save or write script source before adding to a flow.");
  }

  flows.openComposerWithGrapheme(trimmed, scriptName);
  automationsNav.openSection("flows");

  if (layout.isMobile) {
    layout.setMobileTab("you", { bump: true });
    layout.openYou("automations");
    return;
  }

  layout.navigateDesktop("automations", { bump: true });
}
