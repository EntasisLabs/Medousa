<script lang="ts">
  import { onMount } from "svelte";
  import RoomShellOptions from "$lib/components/settings/RoomShellOptions.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { ColorThemeId } from "$lib/types/colorThemes";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { isTauri } from "$lib/window";
  import { isTauriMobilePlatform } from "$lib/platform";
  import {
    queryLiveActivityAvailability,
    type LiveActivityStatus,
  } from "$lib/liveActivity";
  import {
    hostComputerPhrase,
    workshopRetentionLocalHint,
    workshopRetentionReadHint,
  } from "$lib/platformCopy";
  import { Check, ChevronDown, Moon, Sun } from "@lucide/svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const activePreset = $derived(
    environment.spec?.layoutPresets?.find((preset) => preset.active) ??
      environment.spec?.layoutPresets?.find(
        (preset) => preset.id === environment.spec?.activePresetId,
      ) ??
      null,
  );
  const activeLayoutLabel = $derived(
    presetDisplayLabel(activePreset?.id ?? "default", activePreset?.label),
  );
  const activeThemeOption = $derived(
    COLOR_THEME_OPTIONS.find((option) => option.id === settings.colorTheme) ??
      COLOR_THEME_OPTIONS[0]!,
  );

  const prefsHint = $derived.by(() => {
    if (!isTauri()) {
      return "Look, notifications, and how this space behaves.";
    }
    return `${workshops.activeLabel} — look, notifications, and shell chrome.`;
  });

  let themeBusy = $state(false);
  let themePickerOpen = $state(false);
  let moreOpen = $state(false);
  let liveActivityStatus = $state<LiveActivityStatus | null>(null);

  const retentionReadOnly = $derived(mobile || isTauriMobilePlatform());

  async function refreshLiveActivityStatus() {
    if (!mobile && !isTauriMobilePlatform()) return;
    liveActivityStatus = await queryLiveActivityAvailability();
  }

  onMount(() => {
    void refreshLiveActivityStatus();
  });

  function toggleDarkMode() {
    settings.setDarkMode(!settings.darkMode);
  }

  async function pickTheme(themeId: ColorThemeId) {
    if (themeBusy || settings.colorTheme === themeId) {
      themePickerOpen = false;
      return;
    }
    themeBusy = true;
    try {
      await environment.setActiveLayoutColorTheme(themeId);
      themePickerOpen = false;
    } catch {
      settings.setColorTheme(themeId);
      themePickerOpen = false;
    } finally {
      themeBusy = false;
    }
  }

  function commitHideHours(raw: string) {
    settings.setWorkCardHideAfterHours(Number(raw));
    void settings.persistWorkRetention();
  }

  function commitWipeDays(raw: string) {
    settings.setWorkCardWipeAfterDays(Number(raw));
    void settings.persistWorkRetention();
  }
</script>

<section class="settings-section prefs">
  <header class="settings-section-header prefs-header">
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold text-surface-50">Preferences</h2>
      <p class="workshop-faint mt-1 text-sm">{prefsHint}</p>
    </div>
    <button
      type="button"
      class="workshop-rail-btn prefs-mode-toggle shrink-0"
      aria-label={settings.darkMode ? "Switch to light mode" : "Switch to dark mode"}
      title={settings.darkMode ? "Light mode" : "Dark mode"}
      aria-pressed={settings.darkMode}
      onclick={toggleDarkMode}
    >
      {#if settings.darkMode}
        <Sun size={16} strokeWidth={1.75} />
      {:else}
        <Moon size={16} strokeWidth={1.75} />
      {/if}
    </button>
  </header>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Look</h3>
      <p class="settings-subsection-lead">
        Theme for <span class="prefs-layout-name">{activeLayoutLabel}</span>
      </p>
    </div>

    <div class="prefs-stack">
      <div class="prefs-theme">
        <button
          type="button"
          class="prefs-theme-trigger"
          class:prefs-theme-trigger-open={themePickerOpen}
          aria-haspopup="listbox"
          aria-expanded={themePickerOpen}
          disabled={themeBusy}
          onclick={() => (themePickerOpen = !themePickerOpen)}
        >
          <span class="prefs-theme-swatches" aria-hidden="true">
            {#each activeThemeOption.swatches as swatch, index (index)}
              <span style:background-color={swatch}></span>
            {/each}
          </span>
          <span class="prefs-theme-copy">
            <span class="prefs-theme-name">{activeThemeOption.label}</span>
            <span class="workshop-faint prefs-theme-meta">{activeThemeOption.tagline}</span>
          </span>
          <span class="prefs-theme-action workshop-faint">
            {themePickerOpen ? "Close" : "Change"}
          </span>
        </button>

        {#if themePickerOpen}
          <div class="prefs-theme-popover" role="listbox" aria-label="Choose theme">
            {#each COLOR_THEME_OPTIONS as option (option.id)}
              {@const active = settings.colorTheme === option.id}
              <button
                type="button"
                role="option"
                aria-selected={active}
                class="prefs-theme-card"
                class:prefs-theme-card-active={active}
                disabled={themeBusy}
                onclick={() => void pickTheme(option.id)}
              >
                <span class="prefs-theme-card-swatches" aria-hidden="true">
                  {#each option.swatches as swatch, index (index)}
                    <span style:background-color={swatch}></span>
                  {/each}
                </span>
                <span class="prefs-theme-card-copy">
                  <span class="prefs-theme-card-name">{option.label}</span>
                  <span class="prefs-theme-card-meta">{option.tagline}</span>
                </span>
                {#if active}
                  <Check
                    size={14}
                    strokeWidth={2.5}
                    class="prefs-theme-card-check"
                    aria-hidden="true"
                  />
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <RoomShellOptions compact />
    </div>
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Work cards</h3>
      <p class="settings-subsection-lead">
        {#if retentionReadOnly}
          {workshopRetentionReadHint()}
        {:else}
          Done cards leave the board, then archives clear.
          <span class="opacity-80"> {workshopRetentionLocalHint()}</span>
        {/if}
      </p>
    </div>

    <div class="prefs-grid">
      <label class="prefs-tile prefs-tile-metric">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Hide from board</span>
          <span class="prefs-tile-meta">After done</span>
        </span>
        <span class="prefs-metric">
          <input
            type="number"
            min="1"
            max="168"
            inputmode="numeric"
            class="prefs-metric-input"
            value={settings.workCardHideAfterHours}
            disabled={retentionReadOnly}
            aria-label="Hide from board after hours"
            onchange={(event) => commitHideHours((event.currentTarget as HTMLInputElement).value)}
          />
          <span class="prefs-metric-unit">hrs</span>
        </span>
      </label>

      <label class="prefs-tile prefs-tile-metric">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Clear archives</span>
          <span class="prefs-tile-meta">Then wipe for good</span>
        </span>
        <span class="prefs-metric">
          <input
            type="number"
            min="1"
            max="90"
            inputmode="numeric"
            class="prefs-metric-input"
            value={settings.workCardWipeAfterDays}
            disabled={retentionReadOnly}
            aria-label="Clear archives after days"
            onchange={(event) => commitWipeDays((event.currentTarget as HTMLInputElement).value)}
          />
          <span class="prefs-metric-unit">days</span>
        </span>
      </label>
    </div>
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Everyday</h3>
      <p class="settings-subsection-lead">Saved on this device.</p>
    </div>

    <div class="prefs-grid">
      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Work done alerts</span>
          <span class="prefs-tile-meta">Notify when a card finishes</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.notificationsEnabled}
          onchange={(event) =>
            settings.setNotificationsEnabled((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Workshop guidance</span>
          <span class="prefs-tile-meta">Journeys & starter recipes</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.showWorkshopGuidance}
          onchange={(event) =>
            settings.setShowWorkshopGuidance((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Open Web on browse</span>
          <span class="prefs-tile-meta">Jump when she navigates</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.autoOpenWebOnAgentBrowse}
          onchange={(event) =>
            settings.setAutoOpenWebOnAgentBrowse((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      {#if mobile}
        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Remote push</span>
            <span class="prefs-tile-meta">From {hostComputerPhrase()} while closed</span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={settings.remotePushEnabled}
            onchange={(event) =>
              settings.setRemotePushEnabled((event.currentTarget as HTMLInputElement).checked)}
          />
        </label>

        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Live Activity</span>
            <span class="prefs-tile-meta">Lock Screen / Dynamic Island</span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={settings.liveActivityEnabled}
            onchange={async (event) => {
              settings.setLiveActivityEnabled((event.currentTarget as HTMLInputElement).checked);
              await refreshLiveActivityStatus();
            }}
          />
        </label>
      {/if}
    </div>

    {#if mobile && liveActivityStatus}
      <p class="prefs-footnote">
        {#if liveActivityStatus.error}
          Live Activity: {liveActivityStatus.error}
        {:else if liveActivityStatus.active}
          Live Activity active on Lock Screen / Dynamic Island
        {:else if liveActivityStatus.available}
          Live Activity ready — starts when work is in motion
        {:else}
          Live Activity: checking…
        {/if}
      </p>
      <p class="prefs-footnote">
        Home widget: add <strong class="font-medium text-surface-300">Pulse</strong> from the iOS
        gallery.
      </p>
    {/if}
  </div>

  <details class="prefs-more" bind:open={moreOpen}>
    <summary class="prefs-more-summary">
      <span>More display options</span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-grid prefs-more-grid">
      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Technical activity</span>
          <span class="prefs-tile-meta">Job noise & workflow events</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.showTechnicalActivity}
          onchange={(event) =>
            settings.setShowTechnicalActivity((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Engine details</span>
          <span class="prefs-tile-meta">Routing & tool telemetry in chat</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.showEngineDetailsInChat}
          onchange={(event) =>
            settings.setShowEngineDetailsInChat((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Model picker</span>
          <span class="prefs-tile-meta">In the chat composer</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.showChatModelPicker}
          onchange={(event) =>
            settings.setShowChatModelPicker((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Liquid chat</span>
          <span class="prefs-tile-meta">Experimental scene renderer</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={settings.liquidChat}
          onchange={(event) =>
            settings.setLiquidChat((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>
    </div>
  </details>
</section>

<style>
  .prefs-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    width: 100%;
    max-width: none;
  }

  .prefs-mode-toggle {
    margin-top: 0.05rem;
    margin-inline-start: auto;
  }

  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-layout-name {
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-theme {
    min-width: 0;
  }

  .prefs-theme-trigger {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    width: 100%;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .prefs-theme-trigger:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.55);
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .prefs-theme-trigger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prefs-theme-trigger-open {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .prefs-theme-swatches {
    display: flex;
    width: 2.5rem;
    height: 1.65rem;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 0.35rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
  }

  .prefs-theme-swatches span {
    display: block;
    height: 100%;
    flex: 1;
  }

  .prefs-theme-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .prefs-theme-name {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-theme-meta {
    font-size: 0.68rem;
    line-height: 1.3;
  }

  .prefs-theme-action {
    flex-shrink: 0;
    font-size: 0.72rem;
    font-weight: 550;
  }

  .prefs-theme-popover {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    max-height: min(16rem, 42vh);
    margin-top: 0.4rem;
    padding: 0.3rem;
    overflow-y: auto;
    border-radius: var(--theme-rounded-container, 0.75rem);
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.6);
  }

  .prefs-theme-card {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.35rem 0.4rem;
    border: 1px solid transparent;
    border-radius: 0.45rem;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .prefs-theme-card:hover:not(:disabled) {
    background: rgb(var(--color-surface-800) / 0.65);
  }

  .prefs-theme-card-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .prefs-theme-card:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-theme-card-swatches {
    display: flex;
    width: 2.2rem;
    height: 1.35rem;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 0.28rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
  }

  .prefs-theme-card-swatches span {
    display: block;
    height: 100%;
    flex: 1;
  }

  .prefs-theme-card-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.05rem;
  }

  .prefs-theme-card-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.78rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-theme-card-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.65rem;
    color: rgb(var(--color-surface-500));
  }

  :global(.prefs-theme-card-check) {
    flex-shrink: 0;
    color: rgb(var(--color-primary-300));
  }

  .prefs-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--prefs-gap);
  }

  @media (min-width: 720px) {
    .prefs-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    cursor: pointer;
  }

  .prefs-tile:hover {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .prefs-tile-metric {
    cursor: default;
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .prefs-metric {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .prefs-metric-input {
    width: 3.1rem;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.45);
    padding: 0.2rem 0.35rem;
    text-align: right;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-100));
  }

  .prefs-metric-input:disabled {
    opacity: 0.5;
  }

  .prefs-metric-unit {
    font-size: 0.68rem;
    color: rgb(var(--color-surface-500));
  }

  .prefs-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .prefs-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .prefs-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .prefs-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .prefs-switch:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.7);
    outline-offset: 2px;
  }

  .prefs-footnote {
    margin: 0.45rem 0 0;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-500));
  }

  .prefs-more {
    margin-top: 1rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    list-style: none;
  }

  .prefs-more-summary::-webkit-details-marker {
    display: none;
  }

  :global(.prefs-more-chevron) {
    transition: transform 160ms ease;
  }

  .prefs-more[open] :global(.prefs-more-chevron) {
    transform: rotate(180deg);
  }

  .prefs-more-grid {
    padding: 0 0.55rem 0.6rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }
</style>
