<script lang="ts">
  import { onMount } from "svelte";
  import WorkshopShell from "$lib/components/layout/WorkshopShell.svelte";
  import MobileShell from "$lib/components/mobile/MobileShell.svelte";
  import { initMobileNative } from "$lib/mobileNative";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { watchMobileViewport } from "$lib/platform";

  async function openWorkCard(cardId: string) {
    if (layout.isMobile) {
      layout.setMobileTab("work");
    } else {
      workspace.workView = "kanban";
    }
    await workspace.selectCard(cardId);
  }

  onMount(() => {
    const stopViewport = watchMobileViewport((mobile) => layout.setMobile(mobile));
    const stopNative = initMobileNative(openWorkCard);
    return () => {
      stopViewport();
      stopNative();
    };
  });
</script>

{#if layout.isMobile}
  <MobileShell />
{:else}
  <WorkshopShell />
{/if}
