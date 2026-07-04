<script lang="ts">
  import { onMount } from "svelte";
  import WorkshopShell from "$lib/components/layout/WorkshopShell.svelte";
  import MobileShell from "$lib/components/mobile/MobileShell.svelte";
  import CommandSpotlight from "$lib/components/layout/CommandSpotlight.svelte";
  import WizardContainer from "$lib/components/wizard/WizardContainer.svelte";
  import VaultGarageImportWizard from "$lib/components/vault/VaultGarageImportWizard.svelte";
  import VaultContextMenu from "$lib/components/vault/VaultContextMenu.svelte";
  import VaultNoteWorkshop from "$lib/components/vault/VaultNoteWorkshop.svelte";
  import MobileBrowserWorkshop from "$lib/components/mobile/MobileBrowserWorkshop.svelte";
  import ToastHost from "$lib/components/layout/ToastHost.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { initMobileNative } from "$lib/mobileNative";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { applyNativeMobileShellLayout, isTauri, isTauriMobilePlatform, watchMobileViewport } from "$lib/platform";
  import { handoffBrowserShell } from "$lib/utils/browserShellHandoff";
  import { attachAgentBrowserCoord } from "$lib/utils/agentBrowserCoord";
  import { humanBrowserSetMobileShellActive } from "$lib/humanBrowser";
  import BrowserWorkshop from "$lib/components/browser/BrowserWorkshop.svelte";

  $effect(() => {
    void chat.sessionId;
    void chat.draft;
    chat.scheduleDraftPersist();
  });

  function focusChatComposer() {
    layout.navigateDesktop("chat", { bump: true });
    void chat.ensureSessionHydrated();
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }

  async function openWorkCard(cardId: string) {
    if (layout.isMobile) {
      layout.setMobileTab("home");
    } else {
      workspace.workView = "kanban";
    }
    await workspace.selectCard(cardId);
  }

  async function openVaultNote(notePath: string) {
    layout.navigateDesktop("library");
    await vault.openNote(notePath);
  }

  onMount(() => {
    commandSpotlight.closeSpotlight();
    document.querySelectorAll(".command-spotlight-backdrop").forEach((node) => {
      node.closest(".body-portal-host")?.remove() ?? node.remove();
    });

    void wizard.bootstrap();
    const stopViewport = layout.attachViewportTracking();
    if (isTauri()) {
      void humanBrowserSetMobileShellActive(layout.isMobile);
    }
    const stopNativeLayout = applyNativeMobileShellLayout();
    const stopMobileViewport = isTauriMobilePlatform()
      ? () => {
          layout.setMobile(true);
        }
      : watchMobileViewport((mobile) => {
          const wasMobile = layout.isMobile;
          layout.setMobile(mobile);
          if (wasMobile !== mobile) {
            handoffBrowserShell(mobile);
          }
        });
    const stopNative = initMobileNative(openWorkCard, openVaultNote);
    const stopAgentBrowserCoord = attachAgentBrowserCoord();

    const onKeydown = (event: KeyboardEvent) => {
      if (layout.isMobile) return;

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        commandSpotlight.toggleSpotlight();
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
        commandSpotlight.openNotes();
      }
    };
    window.addEventListener("keydown", onKeydown);

    return () => {
      stopNativeLayout();
      stopViewport();
      stopMobileViewport();
      stopNative();
      stopAgentBrowserCoord();
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
  <WorkshopShell onOpenSpotlight={() => commandSpotlight.openSpotlight()} />
{/if}

<CommandSpotlight onFocusChat={focusChatComposer} />

<VaultGarageImportWizard />
<VaultContextMenu />
{#if !layout.isMobile}
  <VaultNoteWorkshop
    onOpenFullChat={() => {
      layout.navigateDesktop("chat", { bump: true });
      void chat.ensureSessionHydrated();
    }}
  />
  <BrowserWorkshop
    onOpenFullChat={() => {
      layout.navigateDesktop("chat", { bump: true });
      void chat.ensureSessionHydrated();
    }}
  />
{:else}
  <MobileBrowserWorkshop
    onOpenFullChat={async () => {
      layout.setMobileTab("chat");
      await chat.ensureSessionHydrated();
    }}
  />
{/if}

<ToastHost />
