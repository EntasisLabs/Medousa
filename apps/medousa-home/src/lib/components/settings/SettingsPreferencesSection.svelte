<script lang="ts">
  import { onMount } from "svelte";
  import RoomShellOptions from "$lib/components/settings/RoomShellOptions.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { ColorThemeId } from "$lib/types/colorThemes";
  import {
    COLOR_THEME_GROUP_LABELS,
    COLOR_THEME_GROUPS,
  } from "$lib/types/colorThemes";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { openGuide } from "$lib/guide/openGuide";
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
  import MedousaMark from "$lib/components/brand/MedousaMark.svelte";
  import { MEDOUSA_MARK_OPTIONS } from "$lib/theme/medousaMarks";
  import {
    readGrammarSettings,
    writeGrammarSettings,
    type GrammarSettings,
  } from "$lib/utils/grammarCheck";

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
  let grammar = $state<GrammarSettings>(readGrammarSettings());

  function patchGrammar(partial: Partial<GrammarSettings>) {
    grammar = { ...grammar, ...partial };
    writeGrammarSettings(grammar);
  }

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
      <button
        type="button"
        class="settings-learn-more"
        onclick={() => void openGuide("themes-customization")}
      >
        Learn more
      </button>
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
            {#each COLOR_THEME_GROUPS as group (group)}
              <div class="prefs-theme-group" role="presentation">
                <p class="prefs-theme-group-label">{COLOR_THEME_GROUP_LABELS[group]}</p>
                <div class="prefs-theme-group-grid">
                  {#each COLOR_THEME_OPTIONS.filter((option) => option.group === group) as option (option.id)}
                    {@const active = settings.colorTheme === option.id}
                    <button
                      type="button"
                      role="option"
                      aria-selected={active}
                      class="prefs-theme-card"
                      class:prefs-theme-card-active={active}
                      style:--preview-canvas={option.swatches[0]}
                      style:--preview-action={option.swatches[1]}
                      style:--preview-raised={option.swatches[2]}
                      disabled={themeBusy}
                      onclick={() => void pickTheme(option.id)}
                    >
                      <span class="prefs-theme-mini" aria-hidden="true">
                        <i class="prefs-theme-mini-rail"></i>
                        <i class="prefs-theme-mini-card"></i>
                        <i class="prefs-theme-mini-line"></i>
                        <i class="prefs-theme-mini-action"></i>
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
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="prefs-mark-picker">
        <div class="prefs-mark-head">
          <span class="prefs-tile-title">Your Medousa</span>
          <span class="prefs-tile-meta">Paired automatically during onboarding; editable anytime.</span>
        </div>
        <div class="prefs-mark-grid" role="listbox" aria-label="Choose Medousa mark">
          {#each MEDOUSA_MARK_OPTIONS as option (option.id)}
            {@const active = settings.medousaMark === option.id}
            <button
              type="button"
              role="option"
              aria-selected={active}
              class="prefs-mark-option"
              class:prefs-mark-option-active={active}
              style:--mark-preview-bg={option.previewBackground}
              onclick={() => settings.setMedousaMark(option.id)}
            >
              <span class="prefs-mark-preview" aria-hidden="true">
                <MedousaMark
                  markId={option.id}
                  darkMode={settings.darkMode}
                  decorative
                />
              </span>
              <span class="prefs-mark-label">{option.label}</span>
            </button>
          {/each}
        </div>
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
      <h3 class="settings-subsection-heading">Notes proofread</h3>
      <p class="settings-subsection-lead">
        Grammar underlines in the editor, via a LanguageTool server. Only note
        text leaves this device — never paths. Off by default; browser
        spellcheck stays on either way.
      </p>
    </div>

    <div class="prefs-grid">
      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Grammar check</span>
          <span class="prefs-tile-meta">Underline + fixes in Build</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={grammar.enabled}
          onchange={(event) =>
            patchGrammar({
              enabled: (event.currentTarget as HTMLInputElement).checked,
            })}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">LanguageTool endpoint</span>
          <span class="prefs-tile-meta">Local server (languagetool.org)</span>
        </span>
        <input
          type="text"
          class="prefs-endpoint-input"
          spellcheck="false"
          placeholder="http://localhost:8081"
          value={grammar.endpoint}
          disabled={!grammar.enabled}
          aria-label="LanguageTool endpoint"
          onchange={(event) =>
            patchGrammar({
              endpoint: (event.currentTarget as HTMLInputElement).value,
            })}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Language</span>
          <span class="prefs-tile-meta">Auto-detect or force</span>
        </span>
        <select
          class="prefs-endpoint-input"
          value={grammar.language}
          disabled={!grammar.enabled}
          aria-label="Grammar check language"
          onchange={(event) =>
            patchGrammar({
              language: (event.currentTarget as HTMLSelectElement).value,
            })}
        >
          <option value="auto">Auto</option>
          <option value="en-US">English (US)</option>
          <option value="en-GB">English (UK)</option>
          <option value="es">Español</option>
          <option value="fr">Français</option>
          <option value="de">Deutsch</option>
          <option value="pt-BR">Português</option>
        </select>
      </label>
    </div>
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Everyday</h3>
      <p class="settings-subsection-lead">Saved on this device.</p>
      <button
        type="button"
        class="settings-learn-more"
        onclick={() => void openGuide("keyboard-flow")}
      >
        Learn more
      </button>
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
    --prefs-tile-radius: var(--theme-container-radius, 0.65rem);
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--theme-border, var(--color-surface-500)) / 0.36);
    --prefs-tile-bg: rgb(var(--theme-card, var(--color-surface-900)) / 0.52);
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
    border-color: rgb(var(--theme-focus) / 0.5);
    background: rgb(var(--theme-selection) / 0.09);
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
    display: grid;
    gap: 0.75rem;
    max-height: min(28rem, 62vh);
    margin-top: 0.4rem;
    padding: 0.3rem;
    overflow-y: auto;
    border-radius: var(--theme-rounded-container, 0.75rem);
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.6);
  }

  .prefs-theme-group {
    display: grid;
    gap: 0.35rem;
  }

  .prefs-theme-group-label {
    margin: 0;
    padding: 0.05rem 0.2rem;
    color: rgb(var(--theme-decorative));
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .prefs-theme-group-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.3rem;
  }

  .prefs-theme-card {
    display: grid;
    grid-template-columns: 4.8rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.4rem;
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
    border-color: rgb(var(--theme-focus) / 0.52);
    background: rgb(var(--theme-selection) / 0.09);
  }

  .prefs-theme-card:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-theme-mini {
    position: relative;
    display: block;
    width: 4.8rem;
    height: 2.85rem;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 0.38rem;
    border: 1px solid color-mix(in srgb, var(--preview-action) 34%, transparent);
    background: var(--preview-canvas);
  }

  .prefs-theme-mini i {
    position: absolute;
    display: block;
  }

  .prefs-theme-mini-rail {
    inset: 0 auto 0 0;
    width: 0.8rem;
    background: var(--preview-raised);
    border-right: 1px solid color-mix(in srgb, var(--preview-action) 26%, transparent);
  }

  .prefs-theme-mini-card {
    inset: 0.45rem 0.4rem 0.45rem 1.2rem;
    border-radius: 0.25rem;
    background: color-mix(in srgb, var(--preview-raised) 88%, var(--preview-canvas));
    border: 1px solid color-mix(in srgb, var(--preview-action) 25%, transparent);
  }

  .prefs-theme-mini-line {
    top: 0.85rem;
    left: 1.55rem;
    width: 1.9rem;
    height: 0.16rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--preview-action) 48%, white);
  }

  .prefs-theme-mini-action {
    right: 0.75rem;
    bottom: 0.75rem;
    width: 0.9rem;
    height: 0.35rem;
    border-radius: 999px;
    background: var(--preview-action);
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

  .prefs-mark-picker {
    display: grid;
    gap: 0.55rem;
  }

  .prefs-mark-head {
    display: grid;
    gap: 0.08rem;
  }

  .prefs-mark-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(5.25rem, 1fr));
    gap: 0.45rem;
  }

  .prefs-mark-option {
    display: grid;
    justify-items: center;
    gap: 0.35rem;
    min-width: 0;
    padding: 0.55rem 0.35rem 0.45rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: rgb(var(--color-surface-300));
  }

  .prefs-mark-option:hover,
  .prefs-mark-option-active {
    border-color: rgb(var(--theme-focus) / 0.58);
    background: rgb(var(--theme-selection) / 0.09);
  }

  .prefs-mark-option-active {
    box-shadow: inset 0 0 0 1px rgb(var(--theme-focus) / 0.22);
  }

  .prefs-mark-preview {
    display: grid;
    place-items: center;
    width: 3.4rem;
    height: 4.25rem;
    padding: 0.35rem;
    border-radius: 0.5rem;
    background: var(--mark-preview-bg);
  }

  .prefs-mark-label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.67rem;
  }

  @media (min-width: 720px) {
    .prefs-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 719px) {
    .prefs-theme-group-grid {
      grid-template-columns: 1fr;
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

  .prefs-endpoint-input {
    flex-shrink: 0;
    width: 11rem;
    max-width: 100%;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.45);
    padding: 0.25rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .prefs-endpoint-input:disabled {
    opacity: 0.5;
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
    background: rgb(var(--theme-action) / 0.9);
  }

  .prefs-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .prefs-switch:focus-visible {
    outline: 2px solid rgb(var(--theme-focus) / 0.72);
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
