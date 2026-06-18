<script lang="ts">
  import type { ProfileShelfEntry } from "$lib/types/profileShelf";
  import { profileKindLabel } from "$lib/utils/profileShelf";

  interface Props {
    entries: ProfileShelfEntry[];
    selectedId: string | null;
    loading: boolean;
    error: string | null;
    profileLabel: string;
    onSelect: (id: string) => void;
  }

  let { entries, selectedId, loading, error, profileLabel, onSelect }: Props = $props();
</script>

<div class="min-h-0">
  {#if loading && entries.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm">Loading who she knows you as…</p>
  {:else if error}
    <p class="px-2 py-4 text-sm text-warning-400">{error}</p>
  {:else if entries.length === 0}
    <div class="px-2 py-6">
      <p class="text-sm leading-relaxed text-surface-300">
        She's still getting to know you.
      </p>
      <p class="workshop-faint mt-2 text-sm leading-relaxed">
        Say something in chat — or use the bar below — and it will land here as a memory she
        carries forward.
      </p>
    </div>
  {:else}
    <p class="context-list-whisper">
      {entries.length} memor{entries.length === 1 ? "y" : "ies"} · {profileLabel}
    </p>
    <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
      {#each entries as entry (entry.id)}
        <li>
          <button
            type="button"
            class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedId ===
            entry.id
              ? 'workshop-list-row-active'
              : ''}"
            onclick={() => onSelect(entry.id)}
          >
            <span class="min-w-0 flex-1">
              <span class="line-clamp-2 text-sm font-medium leading-snug text-surface-100">
                {entry.title}
              </span>
              <span class="context-related-memory-meta mt-1 block truncate">
                {entry.subtitle}
              </span>
            </span>
            <span class="workshop-faint mt-0.5 shrink-0 text-[10px] uppercase tracking-wide">
              {profileKindLabel(entry.kind)}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
