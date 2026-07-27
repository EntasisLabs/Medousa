<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronDown, Minus, Plus } from "@lucide/svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import ModelCatalogSheet from "$lib/components/settings/ModelCatalogSheet.svelte";
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
  import { fallbackSummaryLabel } from "$lib/utils/modelsWorkshopStatus";
  import {
    REASONING_EFFORT_OPTIONS,
    normalizeReasoningEffort,
    reasoningEffortLabel,
    type ReasoningEffortMode,
  } from "$lib/types/reasoningEffort";
  import { runtime } from "$lib/stores/runtime.svelte";

  interface Props {
    catalog: ProvidersListResult | null;
    disabled?: boolean;
    sttReady?: boolean;
    onKeyStatusChange?: () => void | Promise<void>;
  }

  let {
    catalog,
    disabled = false,
    sttReady: _sttReady = false,
    onKeyStatusChange,
  }: Props = $props();

  let pickerTarget = $state<ModelPickerTarget | null>(null);
  let pickerOpen = $state(false);
  let expandedFallback = $state<ProfileKind | null>(null);
  let reasoningOpen = $state(false);
  let moreOpen = $state(false);

  const favorites = $derived(workshopDefaults.favoriteModels());
  const activeReasoning = $derived(
    normalizeReasoningEffort(
      workshopDefaults.draft.reasoningEffort ?? runtime.reasoningEffort,
    ),
  );
  const activeReasoningOption = $derived(
    REASONING_EFFORT_OPTIONS.find((option) => option.id === activeReasoning) ??
      REASONING_EFFORT_OPTIONS[0]!,
  );
  const moreSummary = $derived.by(() => {
    const favCount = favorites.length;
    const bits: string[] = [];
    if (favCount > 0) bits.push(`${favCount} favorite${favCount === 1 ? "" : "s"}`);
    const hasFallback = (["main", "vision", "stt"] as ProfileKind[]).some((profile) => {
      const summary = fallbackSummaryLabel(workshopDefaults.draft, profile, catalog);
      return summary !== "Optional";
    });
    if (hasFallback) bits.push("fallbacks set");
    return bits.length > 0 ? bits.join(" · ") : "Favorites & backups";
  });

  onMount(() => {
    void refreshKeyStatus();
  });

  export async function refreshKeyStatus() {
    await onKeyStatusChange?.();
  }

  async function setReasoningEffort(mode: ReasoningEffortMode) {
    if (disabled || workshopDefaults.saving) return;
    if (activeReasoning === mode) {
      reasoningOpen = false;
      return;
    }
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      reasoningEffort: mode,
    };
    runtime.reasoningEffort = mode;
    await workshopDefaults.saveInferenceProfiles();
    reasoningOpen = false;
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

<div class="models-calm">
  <div class="settings-native-group models-primary">
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

  <div class="models-active mt-3">
    <button
      type="button"
      class="models-active-trigger"
      class:models-active-trigger-open={reasoningOpen}
      aria-expanded={reasoningOpen}
      disabled={disabled || workshopDefaults.saving}
      onclick={() => (reasoningOpen = !reasoningOpen)}
    >
      <span class="models-active-copy">
        <span class="models-active-kicker">Reasoning</span>
        <span class="models-active-title">{reasoningEffortLabel(activeReasoning)}</span>
        <span class="models-active-meta">{activeReasoningOption.hint}</span>
      </span>
      <span class="models-active-action workshop-faint">
        {reasoningOpen ? "Close" : "Change"}
      </span>
    </button>

    {#if reasoningOpen}
      <div class="models-picker" role="listbox" aria-label="Choose reasoning effort">
        {#each REASONING_EFFORT_OPTIONS as option (option.id)}
          <button
            type="button"
            role="option"
            class="models-choice"
            class:models-choice-active={activeReasoning === option.id}
            aria-selected={activeReasoning === option.id}
            disabled={disabled || workshopDefaults.saving}
            title={option.hint}
            onclick={() => void setReasoningEffort(option.id)}
          >
            <span class="models-choice-label">{option.label}</span>
            <span class="models-choice-hint">{option.hint}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <details class="models-more" bind:open={moreOpen}>
    <summary class="models-more-summary">
      <span class="models-more-summary-copy">
        <span>Favorites & fallbacks</span>
        <span class="models-more-summary-meta">{moreSummary}</span>
      </span>
      <ChevronDown size={14} strokeWidth={2} class="models-more-chevron" aria-hidden="true" />
    </summary>
    <div class="models-more-body">
      <section>
        <h4 class="models-more-heading">Favorites</h4>
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

      <section class="mt-4">
        <h4 class="models-more-heading">Fallbacks</h4>
        <div class="settings-native-group">
          {#each (["main", "vision", "stt"] as ProfileKind[]) as profile (profile)}
            {@const summary = fallbackSummaryLabel(workshopDefaults.draft, profile, catalog)}
            <SettingsListRow
              label="{profile === 'main'
                ? 'Chat'
                : profile === 'vision'
                  ? 'Vision'
                  : 'Dictation'} fallbacks"
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
  </details>
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

<style>
  .models-calm {
    --models-gap: 0.5rem;
    --models-radius: 0.65rem;
    --models-pad: 0.55rem 0.75rem;
    --models-min-h: 3.25rem;
    --models-border: rgb(var(--color-surface-500) / 0.32);
    --models-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .models-primary {
    border-radius: var(--models-radius);
    overflow: hidden;
  }

  .models-active {
    display: grid;
    gap: var(--models-gap);
  }

  .models-active-trigger {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: var(--models-min-h);
    padding: var(--models-pad);
    border-radius: var(--models-radius);
    border: 1px solid var(--models-border);
    background: var(--models-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .models-active-trigger:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .models-active-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .models-active-trigger-open {
    border-color: rgb(var(--color-primary-500) / 0.35);
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .models-active-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .models-active-kicker {
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .models-active-title {
    font-size: 0.85rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .models-active-meta {
    font-size: 0.7rem;
    line-height: 1.35;
    color: rgb(var(--color-surface-500));
  }

  .models-active-action {
    flex-shrink: 0;
    font-size: 0.72rem;
    font-weight: 600;
  }

  .models-picker {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--models-gap);
  }

  @media (min-width: 720px) {
    .models-picker {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .models-choice {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-height: var(--models-min-h);
    padding: var(--models-pad);
    border-radius: var(--models-radius);
    border: 1px solid var(--models-border);
    background: var(--models-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .models-choice:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .models-choice:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .models-choice-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-500) / 0.18);
  }

  .models-choice-label {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .models-choice-hint {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .models-more {
    margin-top: 0.75rem;
    border-radius: var(--models-radius);
    border: 1px solid var(--models-border);
    background: var(--models-bg);
  }

  .models-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--models-min-h);
    padding: var(--models-pad);
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    list-style: none;
  }

  .models-more-summary::-webkit-details-marker {
    display: none;
  }

  .models-more-summary-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .models-more-summary-meta {
    font-size: 0.68rem;
    font-weight: 500;
    color: rgb(var(--color-surface-500));
  }

  :global(.models-more-chevron) {
    transition: transform 160ms ease;
  }

  .models-more[open] :global(.models-more-chevron) {
    transform: rotate(180deg);
  }

  .models-more-body {
    padding: 0.65rem 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .models-more-heading {
    margin: 0 0 0.4rem;
    font-size: 0.68rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }
</style>
