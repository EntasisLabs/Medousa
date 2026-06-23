<script lang="ts">
  import { onMount } from "svelte";
  import { X } from "@lucide/svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import type { ProviderCatalogEntry, ProvidersListResult } from "$lib/types/providers";
  import {
    messagingClearSecret,
    messagingSaveSecret,
    messagingSecretStatus,
  } from "$lib/messaging";
  import { refreshModelCatalog } from "$lib/utils/modelCapabilityCatalog";

  interface Props {
    catalog: ProvidersListResult | null;
    disabled?: boolean;
    onKeysChanged?: () => void | Promise<void>;
  }

  let { catalog, disabled = false, onKeysChanged }: Props = $props();

  let keyStatus = $state<Record<string, boolean>>({});
  let editing = $state<ProviderCatalogEntry | null>(null);
  let keyDraft = $state("");
  let savingKey = $state(false);
  let keyMessage = $state<string | null>(null);
  let catalogRefreshing = $state(false);
  let catalogMessage = $state<string | null>(null);

  const providers = $derived(
    catalog?.providers.filter((entry) => entry.id !== "medousa-local") ?? [],
  );

  onMount(() => {
    void refreshStatuses();
  });

  async function refreshStatuses() {
    if (!catalog) return;
    const next: Record<string, boolean> = {};
    await Promise.all(
      catalog.providers.map(async (entry) => {
        if (!entry.needsApiKey) {
          next[entry.id] = true;
          return;
        }
        next[entry.id] = await messagingSecretStatus(`api_key_${entry.id}`);
      }),
    );
    keyStatus = next;
  }

  function openProvider(entry: ProviderCatalogEntry) {
    if (disabled || !entry.needsApiKey) return;
    editing = entry;
    keyDraft = "";
    keyMessage = null;
  }

  async function saveKey() {
    if (!editing) return;
    savingKey = true;
    keyMessage = null;
    try {
      const trimmed = keyDraft.trim();
      if (!trimmed) {
        keyMessage = "Paste a key or use Clear stored key.";
        return;
      }
      await messagingSaveSecret(`api_key_${editing.id}`, trimmed);
      if (editing.id === "openai" || editing.id === "deepseek") {
        await messagingSaveSecret("api_key", trimmed);
      }
      keyStatus = { ...keyStatus, [editing.id]: true };
      keyMessage = "Key stored on this device.";
      editing = null;
      await onKeysChanged?.();
    } catch (err) {
      keyMessage = err instanceof Error ? err.message : String(err);
    } finally {
      savingKey = false;
    }
  }

  async function clearKey() {
    if (!editing) return;
    savingKey = true;
    keyMessage = null;
    try {
      await messagingClearSecret(`api_key_${editing.id}`);
      keyStatus = { ...keyStatus, [editing.id]: false };
      keyMessage = "Key removed.";
      editing = null;
      await onKeysChanged?.();
    } catch (err) {
      keyMessage = err instanceof Error ? err.message : String(err);
    } finally {
      savingKey = false;
    }
  }

  async function refreshCatalog() {
    catalogRefreshing = true;
    catalogMessage = null;
    try {
      const response = await refreshModelCatalog();
      catalogMessage = `Refreshed ${response.refreshed.length} provider catalog(s).`;
    } catch (err) {
      catalogMessage = err instanceof Error ? err.message : String(err);
    } finally {
      catalogRefreshing = false;
    }
  }

  function providerValue(entry: ProviderCatalogEntry): string {
    if (!entry.needsApiKey) return "No key needed";
    return keyStatus[entry.id] ? "Key stored" : "Not set";
  }
</script>

<div class="settings-native-stack">
  <section>
    <h3 class="settings-native-heading">Provider keys</h3>
    <p class="settings-native-footnote">Stored on this device — never sent to Medousa cloud.</p>
    <div class="settings-native-group">
      {#each providers as entry (entry.id)}
        <SettingsListRow
          label={entry.label}
          value={providerValue(entry)}
          hint={entry.needsApiKey ? null : entry.blurb}
          disabled={disabled || !entry.needsApiKey}
          onclick={() => openProvider(entry)}
        />
      {/each}
    </div>
  </section>

  <section>
    <h3 class="settings-native-heading">Catalog</h3>
    <div class="settings-native-group">
      <SettingsListRow
        label="Refresh model catalog"
        value={catalogRefreshing ? "Refreshing…" : "From daemon"}
        disabled={disabled || catalogRefreshing}
        onclick={() => void refreshCatalog()}
      />
    </div>
    {#if catalogMessage}
      <p class="settings-inline-status mt-2">{catalogMessage}</p>
    {/if}
  </section>
</div>

{#if editing}
  <div
    class="model-catalog-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) editing = null;
    }}
  >
    <div class="model-catalog-sheet model-catalog-sheet-narrow" role="dialog" aria-modal="true">
      <header class="model-catalog-sheet-header">
        <div class="min-w-0 flex-1">
          <h3 class="model-catalog-sheet-title">{editing.label} API key</h3>
          <p class="model-catalog-sheet-subtitle">{editing.keyHint ?? "Paste your provider API key."}</p>
        </div>
        <button
          type="button"
          class="model-catalog-sheet-close"
          aria-label="Close"
          onclick={() => (editing = null)}
        >
          <X size={18} />
        </button>
      </header>
      <label class="block px-4 pb-2">
        <span class="settings-native-footnote">API key</span>
        <input
          class="input mt-2 w-full font-mono text-sm"
          type="password"
          autocomplete="off"
          placeholder={editing.keyHint ?? "Paste key"}
          bind:value={keyDraft}
          disabled={savingKey}
        />
      </label>
      {#if keyMessage}
        <p class="settings-inline-status px-4 pb-2">{keyMessage}</p>
      {/if}
      <div class="flex flex-wrap gap-2 px-4 pb-4">
        <button
          type="button"
          class="btn variant-filled-primary btn-sm"
          disabled={savingKey}
          onclick={() => void saveKey()}
        >
          {savingKey ? "Saving…" : "Save key"}
        </button>
        {#if keyStatus[editing.id]}
          <button
            type="button"
            class="btn variant-ghost-surface btn-sm"
            disabled={savingKey}
            onclick={() => void clearKey()}
          >
            Clear stored key
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
