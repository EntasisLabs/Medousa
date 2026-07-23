<script lang="ts">
  import { MessageSquarePlus, Zap } from "@lucide/svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import { dispatchWorkFocusAsk } from "$lib/utils/workChromeEvents";
  import type { WorkCard } from "$lib/types/workspace";

  interface Props {
    onPickCard?: (cardId: string) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickCard, chrome = "rail-list" }: Props = $props();

  const living = $derived(partitionWorkHub(workspace.visibleCards()).living);

  async function pick(card: WorkCard) {
    await workspace.selectCard(card.id);
    onPickCard?.(card.id);
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if living.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      <Zap size={22} strokeWidth={1.5} class="text-surface-500" />
      <p class="text-sm text-surface-300">Nothing in motion</p>
      <button
        type="button"
        class="btn btn-sm btn-primary"
        onclick={() => {
          dispatchWorkFocusAsk();
          onPickCard?.("");
        }}
      >
        <MessageSquarePlus size={14} strokeWidth={2} />
        New ask
      </button>
    </div>
  {:else}
    <ul class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each living as card (card.id)}
        <li>
          <button
            type="button"
            class="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70 {workspace.selectedCardId ===
            card.id
              ? 'bg-surface-800/90'
              : ''}"
            onclick={() => void pick(card)}
          >
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[13px] font-medium text-surface-100">
                {card.title || "Untitled"}
              </span>
              <span class="block truncate text-[11px] text-surface-500">
                {card.status_label || card.column}
              </span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
