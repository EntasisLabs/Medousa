<script lang="ts">
  import { onMount } from "svelte";
  import ModelsSettingsTab from "$lib/components/settings/ModelsSettingsTab.svelte";
  import ProvidersSettingsTab from "$lib/components/settings/ProvidersSettingsTab.svelte";
  import { listProviders, type ProvidersListResult } from "$lib/utils/providersApi";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { composerSttStatus } from "$lib/utils/composerStt";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  type ModelsSectionTab = "models" | "providers";

  let sectionTab = $state<ModelsSectionTab>("models");
  let catalog = $state<ProvidersListResult | null>(null);
  let sttReady = $state(false);
  let modelsTab: ModelsSettingsTab | undefined = $state();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  onMount(() => {
    void bootstrap();
  });

  async function bootstrap() {
    try {
      catalog = await listProviders();
    } catch {
      catalog = null;
    }
    try {
      const stt = await composerSttStatus();
      sttReady = stt.available;
    } catch {
      sttReady = false;
    }
  }

  async function refreshSttAndKeys() {
    const stt = await composerSttStatus();
    sttReady = stt.available;
    await modelsTab?.refreshKeyStatus();
  }
</script>

<section class="settings-section settings-section-models">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Models</h2>
    <p class="workshop-faint mt-1 text-sm">Who answers, sees, and listens.</p>
  </header>

  <div class="settings-segmented mt-4" role="tablist" aria-label="Models settings">
    <button
      type="button"
      role="tab"
      aria-selected={sectionTab === "models"}
      class="settings-segmented-btn {sectionTab === 'models' ? 'settings-segmented-btn-active' : ''}"
      onclick={() => (sectionTab = "models")}
    >
      Models
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={sectionTab === "providers"}
      class="settings-segmented-btn {sectionTab === 'providers' ? 'settings-segmented-btn-active' : ''}"
      onclick={() => (sectionTab = "providers")}
    >
      Providers
    </button>
  </div>

  <div class="mt-5">
    {#if sectionTab === "models"}
      <ModelsSettingsTab
        bind:this={modelsTab}
        {catalog}
        {sttReady}
        disabled={readOnly || workshopDefaults.saving}
        onKeyStatusChange={() => void refreshSttAndKeys()}
      />
    {:else}
      <ProvidersSettingsTab
        {catalog}
        disabled={readOnly || workshopDefaults.saving}
        onKeysChanged={() => void refreshSttAndKeys()}
      />
    {/if}
  </div>

  {#if readOnly}
    <p class="workshop-faint mt-6 text-xs leading-relaxed">
      Model picks are managed on your workshop host.
    </p>
  {/if}
</section>
