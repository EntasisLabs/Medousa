<script lang="ts">
  import { onMount } from "svelte";
  import WorkshopShell from "$lib/components/layout/WorkshopShell.svelte";
  import MobileShell from "$lib/components/mobile/MobileShell.svelte";
  import CommandPalette from "$lib/components/layout/CommandPalette.svelte";
  import WizardContainer from "$lib/components/wizard/WizardContainer.svelte";
  import VaultGarageImportWizard from "$lib/components/vault/VaultGarageImportWizard.svelte";
  import VaultContextMenu from "$lib/components/vault/VaultContextMenu.svelte";
  import VaultQuickSwitcher from "$lib/components/vault/VaultQuickSwitcher.svelte";
  import { initMobileNative } from "$lib/mobileNative";
  import { layout } from "$lib/stores/layout.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { applyNativeMobileShellLayout, isTauriMobilePlatform, watchMobileViewport } from "$lib/platform";

  let commandPaletteOpen = $state(false);
  let quickSwitcherOpen = $state(false);

  async function openWorkCard(cardId: string) {
    if (layout.isMobile) {
      layout.setMobileTab("work");
    } else {
      workspace.workView = "kanban";
    }
    await workspace.selectCard(cardId);
  }

  onMount(() => {
    void wizard.bootstrap();
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
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
        const target = event.target as HTMLElement | null;
        const typing =
          target &&
          (target.tagName === "INPUT" ||
            target.tagName === "TEXTAREA" ||
            target.isContentEditable);
        if (typing) return;
        event.preventDefault();
        quickSwitcherOpen = !quickSwitcherOpen;
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

{#if wizard.loading}
  <div class="flex h-screen items-center justify-center bg-surface-950 text-surface-400">
    <p class="text-sm">Opening your workshop…</p>
  </div>
{:else if wizard.visible}
  <WizardContainer />
{:else if layout.isMobile}
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

<VaultGarageImportWizard />
<VaultContextMenu />
<VaultQuickSwitcher open={quickSwitcherOpen} onClose={() => (quickSwitcherOpen = false)} />
