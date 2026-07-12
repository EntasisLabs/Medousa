<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import {
    HOST_BUS_CHARTER_OPTIONS,
    TOOL_CALL_CHARTER_OPTIONS,
  } from "$lib/types/settings";
  import { WEB_SEARCH_PROVIDER_OPTIONS } from "$lib/types/workshopDefaults";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const preferredProvider = $derived(workshopDefaults.draft.webSearchPreferredProvider ?? "");

  function numField(key: "maxToolRounds", event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function selectField(key: "toolCallMode" | "hostTurnBusMode", value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: value,
    };
  }

  function setWebSearchProvider(value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      webSearchPreferredProvider: value,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Reach</h2>
    <p class="workshop-faint mt-1 text-sm">
      What she may touch — and how she delegates when work gets heavy.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="settings-subsection-heading">Allowed tools</h3>
    <p class="settings-subsection-lead">
      Module names she can call. Leave empty for the full catalog — messaging and delivery live
      elsewhere.
    </p>
    <textarea
      class="reach-tools-input"
      rows="2"
      bind:value={workshopDefaults.allowedModulesText}
      placeholder="websearch.search, fetch.url"
      readonly={readOnly}
      disabled={readOnly}
    ></textarea>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Web search</h3>
    <p class="settings-subsection-lead">Preferred provider when she looks things up.</p>
    <div class="mt-1 grid gap-2 sm:grid-cols-2">
      {#each WEB_SEARCH_PROVIDER_OPTIONS as option (option.value)}
        <button
          type="button"
          class="settings-depth-card {preferredProvider === option.value
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          aria-pressed={preferredProvider === option.value}
          onclick={() => setWebSearchProvider(option.value)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
        </button>
      {/each}
    </div>

    <div class="settings-toggle-list mt-3">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Search fallbacks</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Try other providers when the preferred one fails
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={workshopDefaults.draft.webSearchTryFallbacks ?? true}
          disabled={readOnly}
          onchange={(event) =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              webSearchTryFallbacks: (event.currentTarget as HTMLInputElement).checked,
            })}
        />
      </label>
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Tool posture</h3>
    <p class="settings-subsection-lead">How strictly she invokes tools on a turn.</p>
    <div class="mt-1 grid gap-2 sm:grid-cols-2">
      {#each TOOL_CALL_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.toolCallMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          aria-pressed={workshopDefaults.draft.toolCallMode === option.id}
          onclick={() => selectField("toolCallMode", option.id)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">When to bring in specialists</h3>
    <p class="settings-subsection-lead">
      How often she routes a turn through specialist models.
    </p>
    <div class="mt-1 grid gap-2 sm:grid-cols-3">
      {#each HOST_BUS_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.hostTurnBusMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          aria-pressed={workshopDefaults.draft.hostTurnBusMode === option.id}
          onclick={() => selectField("hostTurnBusMode", option.id)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Tool rounds</h3>
    <p class="settings-subsection-lead">
      How many tool calls she may chain before stopping on one turn.
    </p>
    <div class="settings-toggle-list">
      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Per turn</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Cap on chained tool calls in a single turn
          </span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input"
            min="1"
            max="48"
            inputmode="numeric"
            value={workshopDefaults.draft.maxToolRounds ?? 10}
            readonly={readOnly}
            disabled={readOnly}
            aria-label="Tool rounds per turn"
            oninput={(event) => numField("maxToolRounds", event)}
          />
          <span class="settings-metric-unit" aria-hidden="true">rounds</span>
        </span>
      </label>
    </div>
    <p class="settings-subsection-lead mt-3 mb-0">
      Stage models live in Settings → Models → Stages.
    </p>
  </div>

  <div class="mt-6">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>

<style>
  .reach-tools-input {
    display: block;
    width: 100%;
    resize: vertical;
    min-height: 2.75rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.45);
    padding: 0.55rem 0.7rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .reach-tools-input::placeholder {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .reach-tools-input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  .reach-tools-input:disabled,
  .reach-tools-input:read-only {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
