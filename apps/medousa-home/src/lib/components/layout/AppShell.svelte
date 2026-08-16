<script lang="ts">
  import { onMount } from "svelte";
  import HomeSplash from "$lib/components/layout/HomeSplash.svelte";
  import LazyFeatureView from "$lib/components/layout/LazyFeatureView.svelte";
  import ShellChunkError from "$lib/components/layout/ShellChunkError.svelte";
  import ToastHost from "$lib/components/layout/ToastHost.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
  import { browserWorkshop } from "$lib/stores/browserWorkshop.svelte";
  import { workAskDock } from "$lib/stores/workAskDock.svelte";
  import { vaultOverlay } from "$lib/vault/vaultOverlay.svelte";
  import { vaultContextMenu } from "$lib/stores/vaultContextMenu.svelte";
  import { scriptContextMenu } from "$lib/stores/scriptContextMenu.svelte";
  import { shellContextMenu } from "$lib/stores/shellContextMenu.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { probeClientPlatform } from "$lib/runtime/platformProbe";
  import { startShellRootResources } from "$lib/runtime/shellLifecycle";
  import { focusChatComposer } from "$lib/runtime/shellUseCases";
  import {
    loadBrowserWorkshop,
    loadCommandSpotlight,
    loadMobileBrowserWorkshop,
    loadScriptContextMenu,
    loadShellContextMenu,
    loadVaultAttachmentPanel,
    loadVaultContextMenu,
    loadVaultGarageImportWizard,
    loadVaultNoteWorkshop,
    loadWizardContainer,
    loadWorkAskDockPopover,
  } from "$lib/runtime/viewLoaders";

  const loadDesktopShell = () => import("$lib/components/layout/WorkshopShell.svelte");
  const loadMobileShell = () => import("$lib/components/mobile/MobileShell.svelte");
  const initialPlatform = probeClientPlatform();
  void (initialPlatform === "mobile" ? loadMobileShell() : loadDesktopShell());

  let shellEpoch = $state(0);

  $effect(() => {
    void chat.sessionId;
    void chat.draft;
    chat.scheduleDraftPersist();
  });

  onMount(() => startShellRootResources());
</script>

{#if wizard.loading}
  <HomeSplash />
{:else if wizard.visible && isTauriMobilePlatform()}
  <LazyFeatureView loader={loadWizardContainer} overlay />
{:else if layout.isMobile}
  {#key shellEpoch}
    {#await loadMobileShell()}
      <HomeSplash />
    {:then { default: MobileShell }}
      <MobileShell />
    {:catch}
      <ShellChunkError onRetry={() => { shellEpoch += 1; }} />
    {/await}
  {/key}
{:else}
  {#key shellEpoch}
    {#await loadDesktopShell()}
      <HomeSplash />
    {:then { default: WorkshopShell }}
      <WorkshopShell onOpenSpotlight={() => commandSpotlight.openSpotlight()} />
    {:catch}
      <ShellChunkError onRetry={() => { shellEpoch += 1; }} />
    {/await}
  {/key}
{/if}

{#if commandSpotlight.open}
  <LazyFeatureView
    loader={loadCommandSpotlight}
    overlay
    onFocusChat={focusChatComposer}
  />
{/if}
{#if !layout.isMobile && workAskDock.open}
  <LazyFeatureView loader={loadWorkAskDockPopover} overlay />
{/if}

{#if vaultOverlay.garageWizardOpen}
  <LazyFeatureView loader={loadVaultGarageImportWizard} overlay />
{/if}
{#if vaultContextMenu.open}
  <LazyFeatureView loader={loadVaultContextMenu} overlay />
{/if}
{#if scriptContextMenu.open}
  <LazyFeatureView loader={loadScriptContextMenu} overlay />
{/if}
{#if shellContextMenu.open}
  <LazyFeatureView loader={loadShellContextMenu} overlay />
{/if}
{#if vaultOverlay.attachmentPanelOpen}
  <LazyFeatureView loader={loadVaultAttachmentPanel} overlay />
{/if}
{#if !layout.isMobile && noteWorkshop.open}
  <LazyFeatureView
    loader={loadVaultNoteWorkshop}
    overlay
    onOpenFullChat={focusChatComposer}
  />
{/if}
{#if !layout.isMobile && browserWorkshop.open}
  <LazyFeatureView
    loader={loadBrowserWorkshop}
    overlay
    onOpenFullChat={focusChatComposer}
  />
{:else if layout.isMobile && browserWorkshop.open}
  <LazyFeatureView
    loader={loadMobileBrowserWorkshop}
    overlay
    onOpenFullChat={async () => {
      browserWorkshop.close();
      focusChatComposer();
    }}
  />
{/if}

<ToastHost />
