<script lang="ts">
  import { onMount } from "svelte";
  import WorkshopShell from "$lib/components/layout/WorkshopShell.svelte";
  import MobileShell from "$lib/components/mobile/MobileShell.svelte";
  import CommandPalette from "$lib/components/layout/CommandPalette.svelte";
  import { initMobileNative } from "$lib/mobileNative";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { applyNativeMobileShellLayout, isTauriMobilePlatform, watchMobileViewport } from "$lib/platform";

  let commandPaletteOpen = $state(false);

  async function openWorkCard(cardId: string) {
    if (layout.isMobile) {
      layout.setMobileTab("work");
    } else {
      workspace.workView = "kanban";
    }
    await workspace.selectCard(cardId);
  }

  onMount(() => {
    const stopNativeLayout = applyNativeMobileShellLayout();
    const stopViewport = isTauriMobilePlatform()
      ? () => {
          layout.setMobile(true);
        }
      : watchMobileViewport((mobile) => layout.setMobile(mobile));
    const stopNative = initMobileNative(openWorkCard);

    const onKeydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        commandPaletteOpen = !commandPaletteOpen;
      }
    };
    window.addEventListener("keydown", onKeydown);

    return () => {
      stopNativeLayout();
      stopViewport();
      stopNative();
      window.removeEventListener("keydown", onKeydown);
    };
  });
</script>

{#if layout.isMobile}
  <MobileShell />
{:else}
  <WorkshopShell />
{/if}

<CommandPalette
  open={commandPaletteOpen}
  onClose={() => (commandPaletteOpen = false)}
  onOpenWork={() => {
    workspace.workView = "kanban";
    const blocked = workspace.cards.find((card) => card.column === "blocked");
    if (blocked) void workspace.selectCard(blocked.id);
  }}
/>
