<script lang="ts">
  import { recurring } from "$lib/stores/recurring.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { columnLabel } from "$lib/types/workspace";
  import { KANBAN_COLUMNS } from "$lib/types/work";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenChat: () => void;
    onOpenNote: (path: string) => void;
  }

  let { onSelectCard, onOpenChat, onOpenNote }: Props = $props();

  const needsAttention = $derived(workspace.needsAttentionCount());
  const primaryCard = $derived(workspace.primaryInMotionCard());

  const nextAction = $derived.by(() => {
    if (primaryCard) {
      return {
        kind: "card" as const,
        title: formatCardTitle(primaryCard),
        label: columnLabel(primaryCard.column),
        action: "Continue",
        onClick: () => onSelectCard(primaryCard.id),
      };
    }
    if (needsAttention > 0) {
      return {
        kind: "blocked" as const,
        title: `${needsAttention} need attention`,
        label: "blocked",
        action: "Review",
        onClick: () => layout.setMobileTab("work"),
      };
    }
    if (vault.selectedPath) {
      const title = vaultDisplayTitle(
        vault.labelByPath().get(vault.selectedPath) ?? vault.title,
        vault.selectedPath,
      );
      return {
        kind: "note" as const,
        title,
        label: "last note",
        action: "Open",
        onClick: () => onOpenNote(vault.selectedPath!),
      };
    }
    return {
      kind: "chat" as const,
      title: "Ready when you are",
      label: "pulse",
      action: "Chat",
      onClick: onOpenChat,
    };
  });

  const motionColumns = $derived(
    KANBAN_COLUMNS.filter((column) => column !== "done" && column !== "blocked"),
  );

  const nextSchedule = $derived(recurring.soonestEnabled());
</script>

<section class="flex flex-1 flex-col px-5 pb-6 pt-4">
  <p class="workshop-faint uppercase tracking-widest">{nextAction.label}</p>

  <h1 class="mobile-pulse-title mt-2">{nextAction.title}</h1>

  <button
    type="button"
    class="btn mt-6 w-full variant-filled-primary"
    onclick={nextAction.onClick}
  >
    {nextAction.action}
  </button>

  {#if needsAttention > 0 && nextAction.kind !== "blocked"}
    <button
      type="button"
      class="mt-4 w-full rounded-md border border-warning-500/35 bg-warning-500/10 px-4 py-3 text-left"
      onclick={() => layout.setMobileTab("work")}
    >
      <p class="text-sm font-medium text-warning-200">{needsAttention} need attention</p>
      <p class="workshop-faint mt-0.5">Tap to review on Work</p>
    </button>
  {/if}

  {#if nextSchedule}
    <p class="workshop-faint mt-6">
      Next · {recurring.labelFor(nextSchedule)} at
      {recurring.formatNextRun(nextSchedule.next_run_at_utc)}
    </p>
  {/if}

  <div
    class="mt-8 grid grid-cols-3 gap-2"
    aria-label="In motion columns"
  >
    {#each motionColumns as column (column)}
      <div class="workshop-inset px-3 py-3 text-center">
        <p class="workshop-faint capitalize">{columnLabel(column)}</p>
        <p class="mt-1 text-xl font-semibold tabular-nums text-surface-100">
          {workspace.columnCounts[column] ??
            workspace.cards.filter((card) => card.column === column).length}
        </p>
      </div>
    {/each}
  </div>
</section>
