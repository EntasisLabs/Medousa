<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { columnLabel } from "$lib/types/workspace";
  import { KANBAN_COLUMNS } from "$lib/types/work";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    onOpenWork: () => void;
    onOpenChat: () => void;
    onOpenNote: (path: string) => void;
    onSelectCard: (id: string) => void;
  }

  let { onOpenWork, onOpenChat, onOpenNote, onSelectCard }: Props = $props();

  const needsAttention = $derived(workspace.needsAttentionCount());
  const primaryCard = $derived(workspace.primaryInMotionCard());

  const nextAction = $derived.by(() => {
    if (primaryCard) {
      return {
        kind: "card" as const,
        title: formatCardTitle(primaryCard),
        subtitle: columnLabel(primaryCard.column),
        action: "Continue",
        onClick: () => onSelectCard(primaryCard.id),
      };
    }

    if (vault.selectedPath) {
      return {
        kind: "note" as const,
        title: vaultDisplayTitle(vault.title, vault.selectedPath),
        subtitle: "Last note",
        action: "Open note",
        onClick: () => onOpenNote(vault.selectedPath!),
      };
    }

    return {
      kind: "chat" as const,
      title: "Start a conversation",
      subtitle: "Get started",
      action: "Start chatting",
      onClick: onOpenChat,
    };
  });
</script>

<section class="flex flex-1 flex-col items-center justify-center p-8">
  <div class="w-full max-w-xl">
    <div
      class="rounded-container-token border border-primary-500/25 bg-surface-900/80 p-6"
    >
      <div class="mb-4 h-0.5 w-12 rounded-full bg-primary-500"></div>
      <p class="text-xs text-surface-400">{nextAction.subtitle}</p>
      <h2 class="mt-1 text-xl font-semibold text-surface-100">
        {nextAction.title}
      </h2>
      <p class="mt-3 text-sm text-surface-400">
        {#if nextAction.kind === "card"}
          Pick up where the workshop left off.
        {:else if nextAction.kind === "note"}
          Jump back into your notes.
        {:else}
          What do you want to work on?
        {/if}
      </p>
      <button
        type="button"
        class="btn variant-filled-primary mt-5"
        onclick={nextAction.onClick}
      >
        {nextAction.action}
      </button>
    </div>

    <div class="mt-6 grid grid-cols-3 gap-3">
      {#each KANBAN_COLUMNS.filter((column) => column !== "done" && column !== "blocked") as column (column)}
        <div class="rounded-container-token border border-surface-500/20 bg-surface-900/40 p-3">
          <p class="text-xs capitalize text-surface-500">{columnLabel(column)}</p>
          <p class="mt-1 text-xl font-semibold text-surface-100">
            {workspace.columnCounts[column] ??
              workspace.cards.filter((card) => card.column === column).length}
          </p>
        </div>
      {/each}
    </div>

    {#if needsAttention > 0}
      <div
        class="mt-4 flex items-center justify-between gap-4 rounded-container-token border border-warning-500/20 bg-surface-900/40 p-4"
      >
        <div>
          <p class="text-xs text-surface-500">Needs attention</p>
          <p class="mt-1 text-lg font-medium text-warning-300">
            {needsAttention} blocked
          </p>
        </div>
        <button
          type="button"
          class="btn variant-soft-warning shrink-0"
          onclick={onOpenWork}
        >
          Review
        </button>
      </div>
    {/if}

    <button
      type="button"
      class="btn variant-ghost-surface mt-6 w-full text-sm text-surface-400"
      onclick={onOpenWork}
    >
      Open work board
    </button>
  </div>
</section>
