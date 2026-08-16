<script lang="ts">
  import { onMount } from "svelte";
  import HomeSplash from "$lib/components/layout/HomeSplash.svelte";
  import ShellChunkError from "$lib/components/layout/ShellChunkError.svelte";
  import CommandSpotlight from "$lib/components/layout/CommandSpotlight.svelte";
  import WorkAskDockPopover from "$lib/components/work/WorkAskDockPopover.svelte";
  import WizardContainer from "$lib/components/wizard/WizardContainer.svelte";
  import VaultGarageImportWizard from "$lib/components/vault/VaultGarageImportWizard.svelte";
  import ScriptContextMenu from "$lib/components/automations/ScriptContextMenu.svelte";
  import ShellContextMenu from "$lib/components/shell/ShellContextMenu.svelte";
  import VaultContextMenu from "$lib/components/vault/VaultContextMenu.svelte";
  import VaultNoteWorkshop from "$lib/components/vault/VaultNoteWorkshop.svelte";
  import VaultAttachmentPanel from "$lib/components/vault/VaultAttachmentPanel.svelte";
  import MobileBrowserWorkshop from "$lib/components/mobile/MobileBrowserWorkshop.svelte";
  import ToastHost from "$lib/components/layout/ToastHost.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import BrowserWorkshop from "$lib/components/browser/BrowserWorkshop.svelte";
  import { probeClientPlatform } from "$lib/runtime/platformProbe";
  import { startShellRootResources } from "$lib/runtime/shellLifecycle";
  import { focusChatComposer } from "$lib/runtime/shellUseCases";

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
  <WizardContainer />
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

<CommandSpotlight onFocusChat={focusChatComposer} />
{#if !layout.isMobile}
  <WorkAskDockPopover />
{/if}

<VaultGarageImportWizard />
<VaultContextMenu />
<ScriptContextMenu />
<ShellContextMenu />
<VaultAttachmentPanel />
{#if !layout.isMobile}
  <VaultNoteWorkshop onOpenFullChat={focusChatComposer} />
  <BrowserWorkshop onOpenFullChat={focusChatComposer} />
{:else}
  <MobileBrowserWorkshop
    onOpenFullChat={async () => {
      const { browserWorkshop } = await import("$lib/stores/browserWorkshop.svelte");
      browserWorkshop.close();
      focusChatComposer();
    }}
  />
{/if}

<ToastHost />
