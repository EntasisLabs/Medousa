<script lang="ts">
  import LmeAgentsExplorer from "$lib/components/lme/explorers/LmeAgentsExplorer.svelte";
  import LmeAutomationsExplorer from "$lib/components/lme/explorers/LmeAutomationsExplorer.svelte";
  import LmeFlowsExplorer from "$lib/components/lme/explorers/LmeFlowsExplorer.svelte";
  import LmeDecksExplorer from "$lib/components/lme/explorers/LmeDecksExplorer.svelte";
  import LmeFilesExplorer from "$lib/components/lme/explorers/LmeFilesExplorer.svelte";
  import LmeNotesExplorer from "$lib/components/lme/explorers/LmeNotesExplorer.svelte";
  import LmeScriptsExplorer from "$lib/components/lme/explorers/LmeScriptsExplorer.svelte";
  import LmeExplorerModeBar from "$lib/components/lme/LmeExplorerModeBar.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import {
    familyForLmeExplorerMode,
    type LmeExplorerFamily,
  } from "$lib/utils/lmeExplorerModes";

  interface Props {
    onOpenChat: () => void;
    /** Which door opened this panel — controls the mode strip. */
    family?: LmeExplorerFamily;
  }

  let { onOpenChat, family }: Props = $props();

  const mode = $derived(lmeWorkspace.explorerMode);
  const resolvedFamily = $derived(family ?? familyForLmeExplorerMode(mode));
</script>

<div
  class="lme-side-panel flex h-full min-h-0 w-full flex-col"
  data-debug-label="lme-side-panel"
  data-family={resolvedFamily}
>
  <LmeExplorerModeBar family={resolvedFamily} />
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if mode === "notes"}
      <LmeNotesExplorer />
    {:else if mode === "files"}
      <LmeFilesExplorer />
    {:else if mode === "presentations"}
      <LmeDecksExplorer {onOpenChat} />
    {:else if mode === "scripts"}
      <LmeScriptsExplorer />
    {:else if mode === "agents"}
      <LmeAgentsExplorer {onOpenChat} />
    {:else if mode === "flows"}
      <LmeFlowsExplorer />
    {:else}
      <LmeAutomationsExplorer />
    {/if}
  </div>
</div>
