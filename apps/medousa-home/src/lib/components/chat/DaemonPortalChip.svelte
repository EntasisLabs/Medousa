<script lang="ts">
  import { connection } from "$lib/stores/connection.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    companionTurnRoutingHint,
    connectToWorkshopHint,
    workshopDaemonDefaultsLabel,
    workshopHostBadge,
  } from "$lib/platformCopy";

  interface Props {
    compact?: boolean;
    class?: string;
  }

  let { compact = false, class: className = "" }: Props = $props();

  const detail = $derived.by(() => {
    if (!connection.health?.ok) return connectToWorkshopHint();
    if (runtime.defaultsLoaded) return runtime.modelLabel();
    return workshopDaemonDefaultsLabel();
  });
</script>

<p
  class="daemon-portal-chip {className}"
  title={companionTurnRoutingHint()}
>
  <span class="daemon-portal-chip-badge">{workshopHostBadge()}</span>
  {#if compact}
    <span class="truncate">{detail}</span>
  {:else}
    <span class="min-w-0 truncate">Using {detail}</span>
  {/if}
</p>
