<script lang="ts">
  import {
    AlertTriangle,
    CheckCircle2,
    CircleOff,
    MessageSquarePlus,
    Zap,
  } from "@lucide/svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { findBlockedGroupForCard, prepareBlockedColumn } from "$lib/utils/groupWork";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import { dispatchWorkFocusAsk } from "$lib/utils/workChromeEvents";
  import type { WorkCard } from "$lib/types/workspace";

  interface Props {
    onPickCard?: (cardId: string) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickCard, chrome = "rail-list" }: Props = $props();

  let busy = $state(false);

  const partition = $derived(partitionWorkHub(workspace.visibleCards()));
  const filter = $derived(workspace.workRailFilter);
  const cards = $derived(partition[filter]);
  const blockedDisplay = $derived(prepareBlockedColumn(workspace.cards));

  const emptyCopy = $derived.by((): { title: string; hint?: string } => {
    switch (filter) {
      case "settled":
        return { title: "No settled work" };
      case "failed":
        return { title: "No failed work" };
      case "stopped":
        return { title: "No stopped work" };
      case "stuck":
        return { title: "Nothing stuck" };
      default:
        return {
          title: "Nothing in motion",
          hint: "Start a new ask from the dock",
        };
    }
  });

  async function pick(card: WorkCard) {
    workspace.openHubView();
    await workspace.selectCard(card.id);
    onPickCard?.(card.id);
  }

  async function archiveSettled() {
    if (busy || partition.settled.length === 0) return;
    busy = true;
    try {
      await workspace.archiveTrayCards(partition.settled);
    } finally {
      busy = false;
    }
  }

  async function clearFailed() {
    if (busy || partition.failed.length === 0) return;
    busy = true;
    try {
      await workspace.archiveTerminalTrayCards(partition.failed, "failed");
    } finally {
      busy = false;
    }
  }

  async function clearStopped() {
    if (busy || partition.stopped.length === 0) return;
    busy = true;
    try {
      await workspace.archiveTerminalTrayCards(partition.stopped, "stopped");
    } finally {
      busy = false;
    }
  }

  async function hideStuckGroup() {
    if (busy || partition.stuck.length === 0) return;
    const lead = partition.stuck[0];
    const group = findBlockedGroupForCard(workspace.cards, lead.id);
    if (!group) return;
    busy = true;
    try {
      await workspace.dismissBlockedGroup(group);
    } finally {
      busy = false;
    }
  }

  async function retryStuckGroup() {
    if (busy || partition.stuck.length === 0) return;
    const lead = partition.stuck[0];
    const group = findBlockedGroupForCard(workspace.cards, lead.id);
    if (!group) return;
    busy = true;
    try {
      await workspace.retryBlockedGroup(group);
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if cards.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      {#if filter === "living"}
        <Zap size={22} strokeWidth={1.5} class="text-surface-500" />
      {:else if filter === "settled"}
        <CheckCircle2 size={22} strokeWidth={1.5} class="text-surface-500" />
      {:else if filter === "failed"}
        <AlertTriangle size={22} strokeWidth={1.5} class="text-surface-500" />
      {:else if filter === "stopped"}
        <CircleOff size={22} strokeWidth={1.5} class="text-surface-500" />
      {:else}
        <span class="text-lg font-bold text-surface-500">!</span>
      {/if}
      <p class="text-sm text-surface-300">{emptyCopy.title}</p>
      {#if emptyCopy.hint}
        <p class="text-[11px] text-surface-500">{emptyCopy.hint}</p>
      {/if}
      {#if filter === "living"}
        <button
          type="button"
          class="btn btn-sm btn-primary"
          onclick={() => {
            workspace.openHubView();
            dispatchWorkFocusAsk();
            onPickCard?.("");
          }}
        >
          <MessageSquarePlus size={14} strokeWidth={2} />
          New ask
        </button>
      {/if}
    </div>
  {:else}
    <ul class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each cards as card (card.id)}
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

  {#if filter !== "living" && cards.length > 0}
    <footer class="work-rail-bulk-footer">
      {#if filter === "settled"}
        <button
          type="button"
          class="workshop-text-action text-[11px]"
          disabled={busy}
          onclick={() => void archiveSettled()}
        >
          Archive settled
        </button>
      {:else if filter === "failed"}
        <button
          type="button"
          class="workshop-text-action text-[11px]"
          disabled={busy}
          onclick={() => void clearFailed()}
        >
          Clear failed
        </button>
      {:else if filter === "stopped"}
        <button
          type="button"
          class="workshop-text-action text-[11px]"
          disabled={busy}
          onclick={() => void clearStopped()}
        >
          Clear stopped
        </button>
      {:else if filter === "stuck"}
        <button
          type="button"
          class="workshop-text-action text-[11px]"
          disabled={busy}
          onclick={() => void retryStuckGroup()}
        >
          Retry all
        </button>
        <button
          type="button"
          class="workshop-text-action text-[11px]"
          disabled={busy}
          onclick={() => void hideStuckGroup()}
        >
          Hide stuck
        </button>
        {#if blockedDisplay.overflow > 0}
          <span class="text-[10px] text-surface-500">
            +{blockedDisplay.overflow} more grouped
          </span>
        {/if}
      {/if}
      {#if workspace.cardActionMessage}
        <p class="w-full text-[10px] text-surface-500">{workspace.cardActionMessage}</p>
      {/if}
    </footer>
  {/if}
</div>
