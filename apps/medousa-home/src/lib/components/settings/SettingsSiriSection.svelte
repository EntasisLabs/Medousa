<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { openGuide } from "$lib/guide/openGuide";
  import {
    readSiriPreferences,
    writeSiriPreferences,
    type SiriPreferences,
    type SiriSpeechMode,
  } from "$lib/config/siriPreferences";

  let preferences = $state<SiriPreferences>(readSiriPreferences());
  let busy = $state(false);

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
</script>

<div class="prefs-band">
  <div class="prefs-band-head">
    <h3 class="settings-subsection-heading">Siri</h3>
    <p class="settings-subsection-lead">Voice replies and the chat Siri continues.</p>
    <button type="button" class="settings-learn-more" onclick={() => void openGuide("siri-and-shortcuts")}>
      Learn more
    </button>
  </div>

  <div class="prefs-grid">
    <label class="prefs-tile">
      <span class="prefs-tile-copy">
        <span class="prefs-tile-title">Speak replies</span>
        <span class="prefs-tile-meta">Auto avoids duplicate audio on headphones</span>
      </span>
      <select
        class="prefs-endpoint-input"
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
        <span class="prefs-tile-title">Spoken length</span>
        <span class="prefs-tile-meta">Up to {preferences.maxSpokenCharacters} characters</span>
      </span>
      <input
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
          class="workshop-rail-btn"
          disabled={busy}
          onclick={() => void patch({ defaultWorkshopId: null, defaultSessionId: null })}
        >
          Follow selected
        </button>
      {:else}
        <button type="button" class="workshop-rail-btn" disabled={busy || !chat.sessionId || !chat.workshopScopeId} onclick={useCurrentChat}>
          Use current
        </button>
      {/if}
    </div>

    <label class="prefs-tile">
      <span class="prefs-tile-copy">
        <span class="prefs-tile-title">Fast response model</span>
        <span class="prefs-tile-meta">Optional override for Shared workshops</span>
      </span>
      <input
        class="prefs-endpoint-input"
        value={preferences.fastResponseModel ?? ""}
        maxlength="256"
        placeholder="Use chat model"
        disabled={busy}
        onchange={(event) => void patch({ fastResponseModel: (event.currentTarget as HTMLInputElement).value.trim() || null })}
      />
    </label>
  </div>
</div>
