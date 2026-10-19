<script lang="ts">
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  const mode = $derived(lmeWorkspace.explorerMode);

</script>

<aside
  class="lme-automations-explorer flex h-full min-h-0 w-full flex-col overflow-hidden"
  aria-label="Automations explorer"
>
  {#if mode === "history"}
    <button class="m-3 rounded px-3 py-2 text-sm hover:bg-surface-800" onclick={() => commandSpotlight.openActivity()}>Open Activity</button>
  {:else if mode === "schedules"}
    <AutomationsPanel
      visible={true}
      lmeHosted={true}
      forcedSection="schedules"
      embedded={true}
    />
  {/if}
</aside>
