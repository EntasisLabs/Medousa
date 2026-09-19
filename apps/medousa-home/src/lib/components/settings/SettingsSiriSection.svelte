<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { openGuide } from "$lib/guide/openGuide";
  import {
    readSiriPreferences,
    writeSiriPreferences,
    type SiriPreferences,
    type SiriSpeechMode,
  } from "$lib/config/siriPreferences";
  import {
    nativeLivePreviewEnabled,
    setNativeLivePreviewEnabled,
  } from "$lib/config/liveVoicePreferences";

  let preferences = $state<SiriPreferences>(readSiriPreferences());
  let busy = $state(false);
  let nativeLivePreview = $state(nativeLivePreviewEnabled());

  async function patch(partial: Partial<SiriPreferences>) {
    if (busy) return;
    busy = true;
    try {
      preferences = await writeSiriPreferences(partial);
    } finally {
      busy = false;
    }
  }

  function useCurrentChat() {
    void patch({
      defaultWorkshopId: chat.workshopScopeId || null,
      defaultSessionId: chat.sessionId || null,
    });
  }

  function toggleNativeLivePreview(enabled: boolean) {
    nativeLivePreview = enabled;
    setNativeLivePreviewEnabled(enabled);
  }
</script>

<div class="prefs-band">
  <div class="siri-head">
    <div>
      <h3>Siri</h3>
      <p>Voice replies and the chat Siri continues.</p>
    </div>
    <button type="button" class="siri-guide" onclick={() => void openGuide("siri-and-shortcuts")}>
      Guide
    </button>
  </div>

  <div class="prefs-grid">
    <label class="prefs-tile">
      <span class="prefs-tile-copy">
        <span class="prefs-tile-title">Speak replies</span>
        <span class="prefs-tile-meta">Auto avoids duplicate audio on headphones</span>
      </span>
      <select
        class="siri-select"
        aria-label="Speak Siri replies"
        value={preferences.speechMode}
        disabled={busy}
        onchange={(event) => void patch({ speechMode: (event.currentTarget as HTMLSelectElement).value as SiriSpeechMode })}
      >
        <option value="auto">Auto</option>
        <option value="always">Always</option>
        <option value="never">Never</option>
      </select>
    </label>

    <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Native Live transport</span>
          <span class="prefs-tile-meta">Audio preview only · tool handoffs stay on WebRTC · falls back on startup failure</span>
        </span>
      <input
        type="checkbox"
        class="prefs-switch"
        checked={nativeLivePreview}
        aria-label="Use native Medousa Live transport preview"
        onchange={(event) => toggleNativeLivePreview((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="prefs-tile range-tile">
      <span class="range-head">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Spoken length</span>
          <span class="prefs-tile-meta">Keep spoken answers concise</span>
        </span>
        <span class="range-value">{preferences.maxSpokenCharacters}</span>
      </span>
      <input
        class="siri-range"
        type="range"
        min="80"
        max="1000"
        step="40"
        value={preferences.maxSpokenCharacters}
        disabled={busy}
        aria-label="Maximum spoken reply length"
        onchange={(event) => void patch({ maxSpokenCharacters: Number((event.currentTarget as HTMLInputElement).value) })}
      />
    </label>

    <div class="prefs-tile">
      <span class="prefs-tile-copy">
        <span class="prefs-tile-title">Default chat</span>
        <span class="prefs-tile-meta">{preferences.defaultSessionId ? "Pinned to this chat" : "Follow the selected chat"}</span>
      </span>
      {#if preferences.defaultSessionId}
        <button
          type="button"
          class="siri-action"
          disabled={busy}
          onclick={() => void patch({ defaultWorkshopId: null, defaultSessionId: null })}
        >
          Follow selected
        </button>
      {:else}
        <button type="button" class="siri-action" disabled={busy || !chat.sessionId || !chat.workshopScopeId} onclick={useCurrentChat}>
          Use current
        </button>
      {/if}
    </div>

    <label class="prefs-tile model-tile">
      <span class="prefs-tile-copy">
        <span class="prefs-tile-title">Fast response model</span>
        <span class="prefs-tile-meta">Optional override for Shared workshops</span>
      </span>
      <input
        class="siri-model-input"
        value={preferences.fastResponseModel ?? ""}
        maxlength="256"
        placeholder="Use chat model"
        disabled={busy}
        onchange={(event) => void patch({ fastResponseModel: (event.currentTarget as HTMLInputElement).value.trim() || null })}
      />
    </label>
  </div>
</div>

<style>
  .prefs-band {
    margin-top: 1.25rem;
  }

  .siri-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }

  .siri-head h3 {
    margin: 0 0 0.15rem;
    font-size: 0.82rem;
    font-weight: 650;
    color: rgb(var(--color-surface-100));
  }

  .siri-head p {
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .siri-guide,
  .siri-action {
    flex-shrink: 0;
    border: 1px solid rgb(var(--theme-border, var(--color-surface-500)) / 0.38);
    border-radius: 999px;
    background: rgb(var(--theme-card, var(--color-surface-900)) / 0.52);
    color: rgb(var(--theme-text-secondary));
    font-size: 0.7rem;
    font-weight: 600;
    line-height: 1;
    cursor: pointer;
  }

  .siri-guide {
    padding: 0.4rem 0.65rem;
  }

  .siri-action {
    padding: 0.42rem 0.65rem;
  }

  .siri-guide:hover,
  .siri-action:hover:not(:disabled) {
    border-color: rgb(var(--theme-focus) / 0.48);
    color: rgb(var(--color-surface-100));
  }

  .siri-action:disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .prefs-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--prefs-gap, 0.5rem);
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-width: 0;
    min-height: var(--prefs-tile-min-h, 3.25rem);
    padding: var(--prefs-tile-pad, 0.55rem 0.75rem);
    border: 1px solid var(--prefs-tile-border, rgb(var(--color-surface-500) / 0.36));
    border-radius: var(--prefs-tile-radius, 0.65rem);
    background: var(--prefs-tile-bg, rgb(var(--color-surface-900) / 0.52));
    cursor: pointer;
    transition: border-color 120ms ease, background 120ms ease;
  }

  .prefs-tile:hover {
    border-color: rgb(var(--color-surface-500) / 0.5);
    background: rgb(var(--color-surface-800) / 0.28);
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
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .siri-select,
  .siri-model-input {
    min-width: 0;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-950) / 0.45);
    color: rgb(var(--color-surface-100));
    font-size: 0.75rem;
  }

  .siri-select {
    flex: 0 0 auto;
    width: 6.25rem;
    padding: 0.32rem 0.45rem;
  }

  .siri-model-input {
    width: 100%;
    padding: 0.45rem 0.55rem;
  }

  .siri-select:disabled,
  .siri-model-input:disabled {
    opacity: 0.5;
  }

  .prefs-switch {
    position: relative;
    flex: 0 0 auto;
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

  .prefs-switch:focus-visible,
  .siri-select:focus-visible,
  .siri-model-input:focus-visible,
  .siri-range:focus-visible,
  .siri-guide:focus-visible,
  .siri-action:focus-visible {
    outline: 2px solid rgb(var(--theme-focus) / 0.72);
    outline-offset: 2px;
  }

  .range-tile,
  .model-tile {
    align-items: stretch;
    flex-direction: column;
  }

  .range-head {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 0.75rem;
  }

  .range-value {
    flex-shrink: 0;
    min-width: 2.6rem;
    padding: 0.28rem 0.45rem;
    border-radius: 999px;
    background: rgb(var(--theme-selection) / 0.1);
    color: rgb(var(--theme-text-secondary));
    font-size: 0.68rem;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .siri-range {
    width: 100%;
    margin: 0.15rem 0 0;
    accent-color: rgb(var(--theme-action));
  }

  @media (min-width: 720px) {
    .prefs-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
