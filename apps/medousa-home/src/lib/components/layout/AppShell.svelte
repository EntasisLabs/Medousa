<script lang="ts">
  import { onMount } from "svelte";
  import WorkshopShell from "$lib/components/layout/WorkshopShell.svelte";
  import MobileShell from "$lib/components/mobile/MobileShell.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { watchMobileViewport } from "$lib/platform";

  onMount(() => watchMobileViewport((mobile) => layout.setMobile(mobile)));
</script>

{#if layout.isMobile}
  <MobileShell />
{:else}
  <WorkshopShell />
{/if}
