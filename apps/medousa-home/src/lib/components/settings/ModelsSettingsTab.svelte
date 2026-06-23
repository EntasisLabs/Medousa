<script lang="ts">
  import { onMount } from "svelte";
  import { Minus, Plus } from "@lucide/svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import ModelCatalogSheet from "$lib/components/settings/ModelCatalogSheet.svelte";
  import ModelsWorkshopStatus from "$lib/components/settings/ModelsWorkshopStatus.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { favoriteToPick } from "$lib/utils/modelCatalog";
  import { providerMonogram } from "$lib/utils/chatModelPicker";
  import type { ProvidersListResult } from "$lib/types/providers";
  import type { ModelPickerTarget, ProfileKind } from "$lib/utils/modelAssignment";
  import {
    applyModelSelection,
    fallbackTargets,
    PRIMARY_TARGETS,
    profileForKind,
    rowLabelForTarget,
  } from "$lib/utils/modelAssignment";
  import {
    fallbackSummaryLabel,
    modelsWorkshopStatus,
  } from "$lib/utils/modelsWorkshopStatus";
  import { messagingSecretStatus } from "$lib/messaging";

  interface Props {
    catalog: ProvidersListResult | null;
    disabled?: boolean;
    sttReady?: boolean;
    onKeyStatusChange?: () => void | Promise<void>;
  }

  let {
    catalog,
    disabled = false,
    sttReady = false,
    onKeyStatusChange,
  }: Props = $props();

  let pickerTarget = $state<ModelPickerTarget | null>(null);
  let pickerOpen = $state(false);
  let keyStatus = $state<Record<string, boolean>>({});
  let expandedFallback = $state<ProfileKind | null>(null);

  const favorites = $derived(workshopDefaults.favoriteModels());
  const statusChips = $derived(
    modelsWorkshopStatus(workshopDefaults.draft, catalog, keyStatus, sttReady),
  );

  onMount(() => {
    void refreshKeyStatus();
  });

  export async function refreshKeyStatus() {
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
    onKeyStatusChange?.();
  }

  function openPicker(target: ModelPickerTarget) {
    if (disabled) return;
    pickerTarget = target;
    pickerOpen = true;
  }

  async function handleSelect(
    selection: import("$lib/types/inferenceProfiles").InferenceTarget | null,
  ) {
    if (!pickerTarget) return;
    if (pickerTarget.type === "favorite-add") {
      if (selection) {
        await workshopDefaults.toggleFavorite(selection.provider, selection.model);
      }
      return;
    }
    workshopDefaults.draft = applyModelSelection(
      workshopDefaults.draft,
      pickerTarget,
      selection,
    );
    await workshopDefaults.saveInferenceProfiles();
    await onKeyStatusChange?.();
  }

  function toggleFallbackSection(profile: ProfileKind) {
    expandedFallback = expandedFallback === profile ? null : profile;
  }

  function primaryMonogram(profile: ProfileKind): string | null {
    const p = profileForKind(workshopDefaults.draft, profile);
    return p?.provider ? providerMonogram(p.provider) : null;
  }

  function primaryProviderHint(profile: ProfileKind): string | null {
    const row = rowLabelForTarget(
      workshopDefaults.draft,
      { type: "primary", profile },
      catalog,
    );
    return row.value === "Not set" ? null : row.hint;
  }
</script>

<div class="settings-native-stack">
  <ModelsWorkshopStatus chips={statusChips} />

  <section>
    <h3 class="settings-native-heading">Favorites</h3>
    <div class="settings-native-group">
      {#each favorites as entry (entry.provider + entry.model)}
        {@const pick = favoriteToPick(entry)}
        <div class="settings-native-favorite-row">
          <span class="settings-native-favorite-badge" aria-hidden="true">
            {providerMonogram(entry.provider)}
          </span>
          <span class="settings-native-favorite-copy">
            <span class="settings-native-favorite-name">{pick.label}</span>
            <span class="settings-native-favorite-meta">{pick.hint ?? entry.provider}</span>
          </span>
          <button
            type="button"
            class="settings-native-icon-btn"
            disabled={disabled}
            title="Remove favorite"
            aria-label="Remove {pick.label}"
            onclick={() => void workshopDefaults.toggleFavorite(entry.provider, entry.model)}
          >
            <Minus size={16} />
          </button>
        </div>
      {/each}
      <button
        type="button"
        class="settings-native-row settings-native-row-add"
        disabled={disabled}
        onclick={() => openPicker({ type: "favorite-add" })}
      >
        <Plus size={16} class="settings-native-row-add-icon" />
        <span class="settings-native-row-label">Add favorite</span>
      </button>
    </div>
  </section>

  <section>
    <h3 class="settings-native-heading">Primary</h3>
    <div class="settings-native-group">
      {#each PRIMARY_TARGETS as target, index (`primary-${index}`)}
        {#if target.type === "primary"}
          {@const profile = target.profile}
          {@const row = rowLabelForTarget(workshopDefaults.draft, target, catalog)}
          <SettingsListRow
            label={row.title}
            value={row.value}
            hint={primaryProviderHint(profile)}
            monogram={primaryMonogram(profile)}
            valueAccent={row.value !== "Not set"}
            {disabled}
            onclick={() => openPicker(target)}
          />
        {/if}
      {/each}
    </div>
  </section>

  <section>
    <h3 class="settings-native-heading">Fallbacks</h3>
    <div class="settings-native-group">
      {#each (["main", "vision", "stt"] as ProfileKind[]) as profile (profile)}
        {@const summary = fallbackSummaryLabel(workshopDefaults.draft, profile, catalog)}
        <SettingsListRow
          label="{profile === 'main' ? 'Chat' : profile === 'vision' ? 'Vision' : 'Dictation'} fallbacks"
          value={summary}
          expanded={expandedFallback === profile}
          {disabled}
          onclick={() => toggleFallbackSection(profile)}
        />
        {#if expandedFallback === profile}
          <div class="settings-native-nested">
            {#each fallbackTargets(profile) as target, index (`${profile}-fb-${index}`)}
              {@const row = rowLabelForTarget(workshopDefaults.draft, target, catalog)}
              <SettingsListRow
                label={row.title}
                value={row.value}
                hint={row.hint}
                valueAccent={row.value !== "Not set"}
                {disabled}
                onclick={() => openPicker(target)}
              />
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  </section>
</div>

{#if workshopDefaults.modelsNotice}
  <p
    class="models-save-toast {workshopDefaults.modelsNotice === 'Saved'
      ? 'models-save-toast-ok'
      : ''}"
  >
    {workshopDefaults.modelsNotice}
  </p>
{/if}

<ModelCatalogSheet
  open={pickerOpen}
  target={pickerTarget}
  {catalog}
  onClose={() => {
    pickerOpen = false;
    pickerTarget = null;
  }}
  onSelect={handleSelect}
/>
