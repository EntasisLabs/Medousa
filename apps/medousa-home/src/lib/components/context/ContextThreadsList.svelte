<script lang="ts">
  import type { ContextThreadEntry } from "$lib/types/locus";
  import { tierHumanLabel } from "$lib/utils/contextHuman";

  interface Props {
    search: string;
    entries: ContextThreadEntry[];
    selectedId: string | null;
    loading: boolean;
    error: string | null;
    retrieved: number;
    onSelect: (id: string) => void;
  }

  let {
    search,
    entries,
    selectedId,
    loading,
    error,
    retrieved,
    onSelect,
  }: Props = $props();
</script>

<div class="min-h-0">
  {#if loading && entries.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm">Loading your sessions…</p>
  {:else if error}
    <p class="px-2 py-4 text-sm text-warning-400">{error}</p>
  {:else if entries.length === 0}
    <p class="workshop-muted px-2 py-4 text-sm leading-relaxed">
      {search.trim()
        ? "Nothing matches — try a session name or a phrase you remember."
        : "Nothing on the shelf yet. Moments appear here when she stores session memory."}
    </p>
  {:else}
    <p class="context-list-whisper">
      {retrieved} moment{retrieved === 1 ? "" : "s"} · newest first
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
              {tierHumanLabel(entry.tier)}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
