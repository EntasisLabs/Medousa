<script lang="ts">
  import type { ContextPostureEntry } from "$lib/utils/contextPosture";
  import { postureListWhisper } from "$lib/utils/contextHuman";

  interface Props {
    search: string;
    entries: ContextPostureEntry[];
    selectedId: string | null;
    loading: boolean;
    error: string | null;
    onSelect: (id: string) => void;
  }

  let { search, entries, selectedId, loading, error, onSelect }: Props = $props();
</script>

<div class="min-h-0">
  {#if loading && entries.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm">Loading session moods…</p>
  {:else if error}
    <p class="px-2 py-4 text-sm text-warning-400">{error}</p>
  {:else if entries.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm leading-relaxed">
      {search.trim()
        ? "Nothing matches — try a session name."
        : "No session moods yet. Posture appears when threads carry how you showed up."}
    </p>
  {:else}
    <p class="context-list-whisper">
      {entries.length} session{entries.length === 1 ? "" : "s"} · how you showed up
    </p>
    <ul class="msg-rail-list">
      {#each entries as entry (entry.id)}
        <li>
          <button
            type="button"
            class="msg-rail-row {selectedId === entry.id ? 'msg-rail-row-active' : ''}"
            onclick={() => onSelect(entry.id)}
          >
            <span class="min-w-0 flex-1">
              <span class="truncate text-sm font-medium text-surface-100">
                {entry.title}
              </span>
              <span class="context-related-memory-meta mt-1 block truncate">
                {postureListWhisper(entry.userAvec, entry.threadCount)}
              </span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .msg-rail-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .msg-rail-row {
    display: flex;
    width: 100%;
    align-items: flex-start;
    gap: 0.75rem;
    margin: 0;
    padding: 0.7rem 0.55rem;
    border: none;
    border-radius: 0.65rem;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .msg-rail-row:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.45);
  }

  .msg-rail-row-active {
    background: rgb(var(--color-primary-500) / 0.09);
  }
</style>
