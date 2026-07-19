import { automationsNav } from "$lib/stores/automationsNav.svelte";
import { flows } from "$lib/stores/flows.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

export function promoteScriptToFlow(
  source: string,
  scriptName?: string | null,
  scriptId?: string | null,
): void {
  const trimmed = source.trim();
  if (!trimmed) {
    throw new Error("Save or write script source before adding to a flow.");
  }

  flows.openComposerWithGrapheme(trimmed, scriptName, scriptId);
  lmeWorkspace.focusFlowComposerTab(scriptName?.trim() || "New flow");
  automationsNav.openSection("flows");

  if (layout.isMobile) {
    layout.openMore("automations");
    return;
  }

  layout.navigateDesktop("library", { bump: true });
}
