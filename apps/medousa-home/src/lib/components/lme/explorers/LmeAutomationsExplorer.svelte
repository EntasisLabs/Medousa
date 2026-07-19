<script lang="ts">
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import HistoryPanel from "$lib/components/automations/HistoryPanel.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  const mode = $derived(lmeWorkspace.explorerMode);

  function openFlowsComposer() {
    lmeWorkspace.focusFlowComposerTab();
  }
</script>

<aside
  class="lme-automations-explorer flex h-full min-h-0 w-full flex-col overflow-hidden"
  aria-label="Automations explorer"
>
  {#if mode === "history"}
    <HistoryPanel
      visible={true}
      embedded={true}
      onOpenFlows={openFlowsComposer}
    />
  {:else if mode === "schedules"}
    <AutomationsPanel
      visible={true}
      lmeHosted={true}
      forcedSection="schedules"
      embedded={true}
    />
  {/if}
</aside>
