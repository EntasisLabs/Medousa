<script lang="ts">
  import { onMount } from "svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { loadTuiDefaultsSummary } from "$lib/config";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildMobileModelDropdownOptions,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { listProviders, probeProviders } from "$lib/utils/providersApi";
  import { normalizeFavoriteModels } from "$lib/utils/modelCatalog";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  let options = $state<ChatModelPickOption[]>([]);
  let loading = $state(true);

  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const selectDisabled = $derived(
    disabled || loading || runtime.savingControls || options.length === 0,
  );

  onMount(() => {
    void bootstrap();
  });

  async function bootstrap() {
    loading = true;
    try {
      const [catalog, probe, summary] = await Promise.all([
        listProviders(),
        probeProviders(),
        loadTuiDefaultsSummary().catch(() => null),
      ]);
      let favorites = normalizeFavoriteModels(summary?.favoriteModels);
      if (workshopDefaults.loaded) {
        favorites = workshopDefaults.favoriteModels();
      }
      options = buildMobileModelDropdownOptions(
        catalog,
        probe,
        runtime.provider,
        runtime.model,
        favorites,
      );
    } catch {
      options = [];
    } finally {
      loading = false;
    }
  }

  async function handleChange(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const key = select.value;
    if (!key || key === activeKey || runtime.savingControls) return;
    const option = options.find((entry) => entry.key === key);
    if (!option) return;
    await runtime.applyModel(option.provider, option.model);
  }
</script>

<select
  class="mobile-composer-select mobile-composer-select-model"
  aria-label="Model"
  disabled={selectDisabled}
  value={activeKey}
  onchange={(event) => void handleChange(event)}
>
  {#if loading}
    <option value={activeKey}>Model</option>
  {:else}
    {#each options as option (option.key)}
      <option value={option.key}>{option.label}</option>
    {/each}
  {/if}
</select>
