<script lang="ts">
  import { settings } from "$lib/stores/settings.svelte";

  const options = [
    { title: "Turn updates", detail: "Final answers and completed work", enabled: () => settings.turnUpdatesEnabled, set: settings.setTurnUpdatesEnabled.bind(settings) },
    { title: "Needs your input", detail: "When work is waiting on you", enabled: () => settings.needsInputEnabled, set: settings.setNeedsInputEnabled.bind(settings) },
    { title: "Peer messages", detail: "Messages from connected devices", enabled: () => settings.peerMessagesEnabled, set: settings.setPeerMessagesEnabled.bind(settings) },
    { title: "Reminders", detail: "Calendar and timer reminders", enabled: () => settings.remindersEnabled, set: settings.setRemindersEnabled.bind(settings) },
  ];
</script>

{#each options as option (option.title)}
  <label class="prefs-tile">
    <span class="prefs-tile-copy">
      <span class="prefs-tile-title">{option.title}</span>
      <span class="prefs-tile-meta">{option.detail}</span>
    </span>
    <input
      type="checkbox"
      class="prefs-switch"
      checked={option.enabled()}
      onchange={(event) => option.set((event.currentTarget as HTMLInputElement).checked)}
    />
  </label>
{/each}

<style>
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
</style>
