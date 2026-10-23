<script lang="ts">
  import CreatePanel from "./CreatePanel.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  let { visible = true }: { visible?: boolean } = $props();
  const scope = $derived(activeWorkshopId());
  let documentId = $state<string | null>(null);
  $effect(() => {
    void scope;
    documentId = null;
  });
</script>

{#if visible}
  {#key scope}
    {#key documentId}<CreatePanel
        {documentId}
        workshopId={scope}
        onBrowse={() => (documentId = null)}
        onOpenProject={(id) => (documentId = id)}
      />{/key}
  {/key}
{/if}
