<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Voice</h2>
    <p class="workshop-faint mt-1 text-sm">
      How much reasoning lands on the page — not who powers chat or dictation.
    </p>
  </header>

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
