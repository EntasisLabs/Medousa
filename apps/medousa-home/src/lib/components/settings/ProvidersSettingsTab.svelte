<script lang="ts">
  import { onMount } from "svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import ProviderConfigSheet from "$lib/components/settings/ProviderConfigSheet.svelte";
  import type { ProviderCatalogEntry, ProvidersListResult } from "$lib/types/providers";
  import { refreshModelCatalog } from "$lib/utils/modelCapabilityCatalog";
  import { listProviders } from "$lib/utils/providersApi";
  import {
    formatProviderSettingsSummary,
    loadProviderSettingsSummary,
    type ProviderSettingsSummary,
  } from "$lib/utils/providerSettings";

  interface Props {
    catalog: ProvidersListResult | null;
    disabled?: boolean;
    onKeysChanged?: () => void | Promise<void>;
  }

  let { catalog: catalogProp = null, disabled = false, onKeysChanged }: Props = $props();

  let localCatalog = $state<ProvidersListResult | null>(null);
  const catalog = $derived(catalogProp ?? localCatalog);

  let summaries = $state<Record<string, ProviderSettingsSummary>>({});
  let editing = $state<ProviderCatalogEntry | null>(null);
  let catalogRefreshing = $state(false);
  let catalogMessage = $state<string | null>(null);

  const providers = $derived(catalog?.providers ?? []);

  onMount(() => {
    if (!catalogProp) {
      void listProviders().then((listed) => {
        localCatalog = listed;
      });
    }
  });

  $effect(() => {
    if (!catalog) return;
    void refreshSummaries();
  });

  async function refreshSummaries() {
    if (!catalog) return;
    const next: Record<string, ProviderSettingsSummary> = {};
    await Promise.all(
      catalog.providers.map(async (entry) => {
        next[entry.id] = await loadProviderSettingsSummary(entry);
      }),
    );
    summaries = next;
  }

  function openProvider(entry: ProviderCatalogEntry) {
    if (disabled) return;
    editing = entry;
  }

  async function refreshCatalog() {
    catalogRefreshing = true;
    catalogMessage = null;
    try {
      const response = await refreshModelCatalog();
      catalogMessage = `Refreshed ${response.refreshed.length} provider catalog(s).`;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("404")) {
        catalogMessage =
          "Catalog refresh needs a newer workshop daemon — rebuild and restart medousa_daemon.";
      } else {
        catalogMessage = message;
      }
    } finally {
      catalogRefreshing = false;
    }
  }

  function providerValue(entry: ProviderCatalogEntry): string {
    const summary = summaries[entry.id];
    if (!summary) return "…";
    return formatProviderSettingsSummary(entry, summary);
  }
</script>

<div class="settings-native-stack">
  <section>
    <h3 class="settings-native-heading">Providers</h3>
    <p class="settings-native-footnote">
      Keys and endpoints live here. Model picks only choose provider + model.
    </p>
    <div class="settings-native-group">
      {#each providers as entry (entry.id)}
        <SettingsListRow
          label={entry.label}
          value={providerValue(entry)}
          hint={entry.blurb}
          disabled={disabled}
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
  <ProviderConfigSheet
    entry={editing}
    onClose={() => (editing = null)}
    onSaved={async () => {
      await refreshSummaries();
      await onKeysChanged?.();
    }}
  />
{/if}
