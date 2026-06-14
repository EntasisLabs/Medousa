<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import ProviderPicker from "$lib/components/settings/ProviderPicker.svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import { findCatalogProvider, type ProvidersListResult } from "$lib/utils/providersApi";
  import { providerMonogram } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";

  interface Props {
    title: string;
    subtitle: string;
    catalog: ProvidersListResult | null;
    providerId: string;
    model: string;
    baseUrl?: string;
    apiKey?: string;
    apiKeySet?: boolean;
    quickProviderIds: string[];
    excludeProviderIds?: string[];
    statusOk: boolean;
    statusLabel: string;
    statusDetail?: string | null;
    disabled?: boolean;
    onProviderChange: (id: string, entry: ProviderCatalogEntry) => void;
    onModelChange: (model: string) => void;
    onApiKeyChange?: (key: string) => void;
    onBaseUrlChange?: (url: string) => void;
    onStatus?: (message: string | null, ok?: boolean) => void;
  }

  let {
    title,
    subtitle,
    catalog,
    providerId,
    model,
    baseUrl = "",
    apiKey = "",
    apiKeySet = false,
    quickProviderIds,
    excludeProviderIds = [],
    statusOk,
    statusLabel,
    statusDetail = null,
    disabled = false,
    onProviderChange,
    onModelChange,
    onApiKeyChange,
    onBaseUrlChange,
    onStatus,
  }: Props = $props();

  let advancedOpen = $state(false);
  let keyDraftOpen = $state(false);

  const selected = $derived(
    catalog ? findCatalogProvider(catalog, providerId) : undefined,
  );

  const quickEntries = $derived.by(() => {
    if (!catalog) return [];
    return quickProviderIds
      .map((id) => findCatalogProvider(catalog, id))
      .filter((entry): entry is ProviderCatalogEntry => !!entry);
  });

  const displayModel = $derived(formatModelDisplayName(model, 28));
  const providerLabel = $derived(selected?.label ?? providerId);
  const needsKey = $derived(selected?.needsApiKey ?? true);

  function selectQuick(entry: ProviderCatalogEntry) {
    onProviderChange(entry.id, entry);
    onStatus?.(null);
  }
</script>

<article class="settings-profile-card">
  <header class="settings-profile-header">
    <div class="min-w-0">
      <h3 class="settings-profile-title">{title}</h3>
      <p class="settings-profile-subtitle">{subtitle}</p>
    </div>
    <span
      class="settings-profile-status {statusOk
        ? 'settings-profile-status-ok'
        : 'settings-profile-status-warn'}"
    >
      {statusLabel}
    </span>
  </header>

  <div class="settings-profile-current">
    <span class="settings-profile-badge" aria-hidden="true">{providerMonogram(providerId)}</span>
    <div class="min-w-0 flex-1">
      <p class="settings-profile-model">{displayModel}</p>
      <p class="settings-profile-provider">{providerLabel}</p>
    </div>
  </div>

  {#if statusDetail}
    <p class="settings-profile-detail">{statusDetail}</p>
  {/if}

  {#if quickEntries.length > 0}
    <div class="settings-profile-quick">
      <span class="settings-profile-quick-label">Quick switch</span>
      <div class="settings-profile-quick-row">
        {#each quickEntries as entry (entry.id)}
          <button
            type="button"
            class="settings-profile-quick-btn {providerId === entry.id
              ? 'settings-profile-quick-btn-active'
              : ''}"
            disabled={disabled}
            onclick={() => selectQuick(entry)}
          >
            {entry.label}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if needsKey && onApiKeyChange}
    <div class="settings-profile-key">
      {#if apiKeySet && !keyDraftOpen && !apiKey.trim()}
        <div class="settings-profile-key-row">
          <span class="settings-profile-key-label">API key stored on this device</span>
          <button
            type="button"
            class="settings-profile-key-action"
            disabled={disabled}
            onclick={() => {
              keyDraftOpen = true;
            }}
          >
            Replace
          </button>
        </div>
      {:else}
        <label class="block">
          <span class="settings-profile-key-label">API key</span>
          <input
            class="settings-profile-key-input"
            type="password"
            autocomplete="off"
            placeholder={selected?.keyHint ?? "Paste key"}
            value={apiKey}
            disabled={disabled}
            oninput={(event) => onApiKeyChange((event.currentTarget as HTMLInputElement).value)}
          />
        </label>
      {/if}
    </div>
  {/if}

  <button
    type="button"
    class="settings-advanced-toggle"
    aria-expanded={advancedOpen}
    disabled={disabled}
    onclick={() => {
      advancedOpen = !advancedOpen;
    }}
  >
    <span>Advanced setup</span>
    <ChevronDown size={14} class="settings-advanced-chevron {advancedOpen ? 'is-open' : ''}" />
  </button>

  {#if advancedOpen}
    <div class="settings-advanced-panel">
      <ProviderPicker
        {providerId}
        {model}
        {apiKey}
        {baseUrl}
        {disabled}
        compact
        {excludeProviderIds}
        onProviderChange={onProviderChange}
        onModelChange={onModelChange}
        {onApiKeyChange}
        {onBaseUrlChange}
        {onStatus}
      />
    </div>
  {/if}
</article>
