<script lang="ts">
  import { X } from "@lucide/svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import {
    messagingClearSecret,
    messagingSaveSecret,
    messagingSecretStatus,
  } from "$lib/messaging";
  import {
    CUSTOM_PROVIDER_CATALOG_ID,
    isValidBaseUrl,
    normalizeBaseUrl,
    normalizeCustomProviderId,
  } from "$lib/utils/customProvider";
  import {
    apiKeySecretId,
    loadCustomProviderId,
    loadProviderBaseUrlOverride,
    providerAllowsApiKey,
    providerAllowsBaseUrl,
    saveCustomProviderId,
    saveProviderBaseUrlOverride,
  } from "$lib/utils/providerSettings";

  interface Props {
    entry: ProviderCatalogEntry;
    onClose: () => void;
    onSaved: () => void | Promise<void>;
  }

  let { entry, onClose, onSaved }: Props = $props();

  let customProviderIdDraft = $state("");
  let baseUrlDraft = $state("");
  let keyDraft = $state("");
  let saving = $state(false);
  let message = $state<string | null>(null);
  let hadKey = $state(false);

  const isCustom = $derived(entry.id === CUSTOM_PROVIDER_CATALOG_ID);

  $effect(() => {
    void loadDrafts();
  });

  async function loadDrafts() {
    message = null;
    if (isCustom) {
      customProviderIdDraft = (await loadCustomProviderId()) ?? "";
    } else {
      customProviderIdDraft = "";
    }
    baseUrlDraft =
      (await loadProviderBaseUrlOverride(entry.id)) ??
      entry.defaultBaseUrl ??
      "";
    keyDraft = "";
    hadKey = providerAllowsApiKey(entry)
      ? await messagingSecretStatus(
          apiKeySecretId(isCustom ? customProviderIdDraft || "custom" : entry.id),
        )
      : false;
  }

  async function save() {
    saving = true;
    message = null;
    try {
      if (isCustom) {
        const providerId = normalizeCustomProviderId(customProviderIdDraft);
        if (!providerId) {
          message = "Enter a provider id (e.g. openai, vllm).";
          return;
        }
        await saveCustomProviderId(providerId);
        customProviderIdDraft = providerId;
      }

      if (providerAllowsBaseUrl(entry)) {
        const trimmed = baseUrlDraft.trim();
        if (trimmed && !isValidBaseUrl(trimmed)) {
          message = "Enter a valid http(s) API base URL.";
          return;
        }
        if (isCustom && !trimmed) {
          message = "Custom providers require an API base URL.";
          return;
        }
        await saveProviderBaseUrlOverride(
          entry.id,
          trimmed ? normalizeBaseUrl(trimmed) : null,
        );
      }

      if (providerAllowsApiKey(entry)) {
        const keySecret = apiKeySecretId(
          isCustom ? normalizeCustomProviderId(customProviderIdDraft) || "custom" : entry.id,
        );
        const trimmedKey = keyDraft.trim();
        if (trimmedKey) {
          await messagingSaveSecret(keySecret, trimmedKey);
          if (entry.id === "openai" || entry.id === "deepseek") {
            await messagingSaveSecret("api_key", trimmedKey);
          }
        }
      }

      message = "Saved on this device.";
      await onSaved();
      onClose();
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function clearKey() {
    saving = true;
    message = null;
    try {
      const keySecret = apiKeySecretId(
        isCustom ? normalizeCustomProviderId(customProviderIdDraft) || "custom" : entry.id,
      );
      await messagingClearSecret(keySecret);
      keyDraft = "";
      hadKey = false;
      message = "Key removed.";
      await onSaved();
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<div
  class="model-catalog-backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div class="model-catalog-sheet model-catalog-sheet-narrow" role="dialog" aria-modal="true">
    <header class="model-catalog-sheet-header">
      <div class="min-w-0 flex-1">
        <h3 class="model-catalog-sheet-title">{entry.label}</h3>
        <p class="model-catalog-sheet-subtitle">{entry.blurb}</p>
      </div>
      <button type="button" class="model-catalog-sheet-close" aria-label="Close" onclick={onClose}>
        <X size={18} />
      </button>
    </header>

    <div class="model-catalog-custom-form">
      {#if isCustom}
        <label class="model-catalog-custom-field">
          <span class="model-catalog-custom-label">Provider id</span>
          <span class="model-catalog-custom-hint">Genai adapter name sent at inference time.</span>
          <input
            type="text"
            class="model-catalog-manual-input"
            placeholder="openai"
            bind:value={customProviderIdDraft}
            autocapitalize="off"
            autocomplete="off"
            spellcheck="false"
            disabled={saving}
          />
        </label>
      {/if}

      {#if providerAllowsBaseUrl(entry)}
        <label class="model-catalog-custom-field">
          <span class="model-catalog-custom-label">API base URL</span>
          <span class="model-catalog-custom-hint">
            {#if entry.id === "ollama"}
              Local Ollama, Ollama Cloud, or any OpenAI-compatible Ollama endpoint.
            {:else if entry.id === "medousa-local"}
              Medousa Engine OpenAI-compatible endpoint on this device.
            {:else if isCustom}
              Required — usually ends in /v1.
            {:else}
              Override the default endpoint if needed.
            {/if}
          </span>
          <input
            type="url"
            class="model-catalog-manual-input"
            placeholder={entry.defaultBaseUrl ?? "https://…"}
            bind:value={baseUrlDraft}
            autocapitalize="off"
            autocomplete="off"
            spellcheck="false"
            disabled={saving}
          />
        </label>
      {/if}

      {#if providerAllowsApiKey(entry)}
        <label class="model-catalog-custom-field">
          <span class="model-catalog-custom-label">API key</span>
          <span class="model-catalog-custom-hint">
            {isCustom ? "Optional for local endpoints — required for most hosted APIs." : (entry.keyHint ? `Example: ${entry.keyHint}` : "Stored on this device only.")}
          </span>
          <input
            type="password"
            class="model-catalog-manual-input"
            placeholder={hadKey ? "••••••••  (leave blank to keep)" : entry.keyHint ?? "Paste key"}
            bind:value={keyDraft}
            autocomplete="off"
            disabled={saving}
          />
        </label>
      {/if}

      {#if message}
        <p class="model-catalog-custom-error">{message}</p>
      {/if}

      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="model-catalog-manual-btn"
          disabled={saving}
          onclick={() => void save()}
        >
          {saving ? "Saving…" : "Save"}
        </button>
        {#if providerAllowsApiKey(entry) && hadKey}
          <button
            type="button"
            class="btn variant-ghost-surface btn-sm"
            disabled={saving}
            onclick={() => void clearKey()}
          >
            Clear key
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>
