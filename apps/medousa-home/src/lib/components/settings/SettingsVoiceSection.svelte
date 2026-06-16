<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import {
    BUILTIN_VOICE_PRESETS,
    MAX_CUSTOM_VOICE_PRESETS,
    normalizeCustomVoicePresets,
    uniqueVoicePresetId,
    type VoicePreset,
  } from "$lib/types/voicePresets";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  let editorOpen = $state(false);
  let editingId = $state<string | null>(null);
  let draftName = $state("");
  let draftDescription = $state("");
  let draftAppendix = $state("");

  const customPresets = $derived(
    normalizeCustomVoicePresets(workshopDefaults.draft.customVoicePresets),
  );
  const allPresets = $derived([...BUILTIN_VOICE_PRESETS, ...customPresets]);
  const activeVoiceId = $derived(
    workshopDefaults.draft.activeVoiceId?.trim() || "default",
  );
  const canAddCustom = $derived(customPresets.length < MAX_CUSTOM_VOICE_PRESETS);

  function resetEditor() {
    editorOpen = false;
    editingId = null;
    draftName = "";
    draftDescription = "";
    draftAppendix = "";
  }

  function openCreateEditor() {
    if (readOnly || !canAddCustom) return;
    editingId = null;
    draftName = "";
    draftDescription = "";
    draftAppendix = "";
    editorOpen = true;
  }

  function openEditEditor(preset: VoicePreset) {
    if (readOnly || preset.builtin) return;
    editingId = preset.id;
    draftName = preset.name;
    draftDescription = preset.description ?? "";
    draftAppendix = preset.voiceAppendix;
    editorOpen = true;
  }

  function setActiveVoice(voiceId: string) {
    if (readOnly || workshopDefaults.saving) return;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      activeVoiceId: voiceId,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
  }

  function deleteCustomPreset(voiceId: string) {
    if (readOnly || workshopDefaults.saving) return;
    const nextCustom = customPresets.filter((preset) => preset.id !== voiceId);
    const nextActive =
      activeVoiceId === voiceId ? "default" : activeVoiceId;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      customVoicePresets: nextCustom,
      activeVoiceId: nextActive,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
    if (editingId === voiceId) resetEditor();
  }

  function saveEditor() {
    if (readOnly || workshopDefaults.saving) return;
    const name = draftName.trim();
    const voiceAppendix = draftAppendix.trim();
    if (!name || !voiceAppendix) return;

    let nextCustom = [...customPresets];
    if (editingId) {
      nextCustom = nextCustom.map((preset) =>
        preset.id === editingId
          ? {
              ...preset,
              name,
              description: draftDescription.trim() || undefined,
              voiceAppendix,
            }
          : preset,
      );
    } else {
      const ids = new Set(nextCustom.map((preset) => preset.id));
      const id = uniqueVoicePresetId(name, ids);
      nextCustom = [
        ...nextCustom,
        {
          id,
          name,
          description: draftDescription.trim() || undefined,
          voiceAppendix,
        },
      ].slice(0, MAX_CUSTOM_VOICE_PRESETS);
    }

    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      customVoicePresets: nextCustom,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
    resetEditor();
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Voice</h2>
    <p class="workshop-faint mt-1 text-sm">
      How she speaks and how much reasoning lands — not who powers chat or dictation.
    </p>
  </header>

  <div class="settings-profile-card mt-5">
    <header class="settings-profile-header">
      <div class="min-w-0">
        <h3 class="settings-profile-title">Stance</h3>
        <p class="settings-profile-subtitle">
          Composer voice presets — shared memory, different tone. Also in the chat toolbar.
        </p>
      </div>
    </header>

    <div class="settings-choice-segment mt-4" role="group" aria-label="Voice stance">
      {#each allPresets as preset (preset.id)}
        <button
          type="button"
          class="settings-choice-segment-btn {activeVoiceId === preset.id
            ? 'settings-choice-segment-btn-active'
            : ''}"
          disabled={readOnly || workshopDefaults.saving}
          aria-pressed={activeVoiceId === preset.id}
          title={preset.description}
          onclick={() => setActiveVoice(preset.id)}
        >
          <span class="settings-choice-segment-label">{preset.name}</span>
          {#if preset.description}
            <span class="settings-choice-segment-hint">{preset.description}</span>
          {/if}
        </button>
      {/each}
    </div>

    {#if customPresets.length > 0 && !readOnly}
      <ul class="mt-4 space-y-2">
        {#each customPresets as preset (preset.id)}
          <li class="flex items-center justify-between gap-3 rounded-lg border border-surface-500/35 px-3 py-2">
            <div class="min-w-0">
              <p class="text-sm font-medium text-surface-100">{preset.name}</p>
              {#if preset.description}
                <p class="text-xs text-surface-400">{preset.description}</p>
              {/if}
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <button
                type="button"
                class="settings-favorites-remove"
                disabled={workshopDefaults.saving}
                onclick={() => openEditEditor(preset)}
              >
                Edit
              </button>
              <button
                type="button"
                class="settings-favorites-remove"
                disabled={workshopDefaults.saving}
                onclick={() => deleteCustomPreset(preset.id)}
              >
                Delete
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if !readOnly}
      <div class="mt-4 flex flex-wrap items-center gap-3">
        {#if !editorOpen}
          <button
            type="button"
            class="btn btn-sm variant-soft"
            disabled={!canAddCustom || workshopDefaults.saving}
            onclick={openCreateEditor}
          >
            Add custom voice
          </button>
          {#if !canAddCustom}
            <span class="text-xs text-surface-400">Up to {MAX_CUSTOM_VOICE_PRESETS} custom voices.</span>
          {/if}
        {/if}
      </div>

      {#if editorOpen}
        <div class="mt-4 space-y-3 rounded-lg border border-surface-500/35 p-4">
          <label class="block">
            <span class="workshop-label">Name</span>
            <input
              class="input mt-1 w-full"
              bind:value={draftName}
              maxlength={40}
              placeholder="Briefings"
            />
          </label>
          <label class="block">
            <span class="workshop-label">Description</span>
            <input
              class="input mt-1 w-full"
              bind:value={draftDescription}
              maxlength={120}
              placeholder="Optional one-liner"
            />
          </label>
          <label class="block">
            <span class="workshop-label">Stance</span>
            <textarea
              class="textarea mt-1 min-h-24 w-full resize-y"
              bind:value={draftAppendix}
              maxlength={600}
              placeholder="How she should answer in this voice…"
            ></textarea>
          </label>
          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              class="btn btn-sm variant-filled-primary"
              disabled={workshopDefaults.saving || !draftName.trim() || !draftAppendix.trim()}
              onclick={saveEditor}
            >
              {editingId ? "Save changes" : "Create voice"}
            </button>
            <button
              type="button"
              class="btn btn-sm variant-soft"
              disabled={workshopDefaults.saving}
              onclick={resetEditor}
            >
              Cancel
            </button>
          </div>
        </div>
      {/if}
    {/if}
  </div>

  <div class="settings-profile-card mt-5">
    <header class="settings-profile-header">
      <div class="min-w-0">
        <h3 class="settings-profile-title">Answer depth</h3>
        <p class="settings-profile-subtitle">
          Applies to every chat turn — shared with the TUI and CLI.
        </p>
      </div>
    </header>

    <div class="settings-choice-segment mt-4" role="group" aria-label="Answer depth">
      {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-choice-segment-btn {workshopDefaults.draft.responseDepthMode === option.id
            ? 'settings-choice-segment-btn-active'
            : ''}"
          disabled={readOnly || workshopDefaults.saving}
          aria-pressed={workshopDefaults.draft.responseDepthMode === option.id}
          title={option.hint}
          onclick={() =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              responseDepthMode: option.id,
            })}
        >
          <span class="settings-choice-segment-label">{option.label}</span>
          <span class="settings-choice-segment-hint">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <p class="settings-profile-footnote mt-4">
    Models and dictation live under
    <span class="text-surface-300">Models</span>
    in the sidebar.
  </p>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
