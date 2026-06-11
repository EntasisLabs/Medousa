<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  function textField(key: "provider" | "model", event: Event) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: (event.currentTarget as HTMLInputElement).value,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Voice</h2>
    <p class="workshop-faint mt-1 text-sm">
      Who speaks for the workshop — and how much depth you want in every answer.
    </p>
  </header>

  <div class="mt-5">
    <span class="block text-sm font-medium text-surface-100">Response depth</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      Applies to chat turns — shared with the TUI and CLI.
    </span>
    <div class="mt-3 grid gap-2 sm:grid-cols-3">
      {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.responseDepthMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          onclick={() =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              responseDepthMode: option.id,
            })}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="mt-6 grid gap-4 sm:grid-cols-2">
    <label class="block">
      <span class="block text-sm font-medium text-surface-100">Provider</span>
      <span class="workshop-faint mt-0.5 block text-xs">Who runs the model — e.g. ollama, openai</span>
      <input
        class="input mt-2 w-full"
        value={workshopDefaults.draft.provider ?? ""}
        placeholder="ollama"
        readonly={readOnly}
        disabled={readOnly}
        oninput={(event) => textField("provider", event)}
      />
    </label>
    <label class="block">
      <span class="block text-sm font-medium text-surface-100">Model</span>
      <span class="workshop-faint mt-0.5 block text-xs">The voice she uses for orchestration</span>
      <input
        class="input mt-2 w-full"
        value={workshopDefaults.draft.model ?? ""}
        placeholder="qwen2.5:7b"
        readonly={readOnly}
        disabled={readOnly}
        oninput={(event) => textField("model", event)}
      />
    </label>
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
