<script lang="ts">
  import { recurring } from "$lib/stores/recurring.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { columnLabel } from "$lib/types/workspace";
  import { KANBAN_COLUMNS } from "$lib/types/work";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import {
    journalDailyHeroTitle,
    resolveJournalDailyHeroPath,
  } from "$lib/utils/vaultNoteBridge";

  interface Props {
    onOpenWork: () => void;
    onOpenChat: () => void;
    onOpenNote: (path: string) => void;
    onSelectCard: (id: string) => void;
  }

  let { onOpenWork, onOpenChat, onOpenNote, onSelectCard }: Props = $props();

  const needsAttention = $derived(workspace.needsAttentionCount());
  const primaryCard = $derived(workspace.primaryInMotionCard());
  const journalDailyPath = $derived(resolveJournalDailyHeroPath(vault.notes));

  const nextAction = $derived.by(() => {
    if (primaryCard) {
      return {
        kind: "card" as const,
        title: formatCardTitle(primaryCard),
        metric: null as string | null,
        label: columnLabel(primaryCard.column),
        action: "Continue",
        onClick: () => onSelectCard(primaryCard.id),
      };
    }

    if (needsAttention > 0) {
      return {
        kind: "blocked" as const,
        title: null as string | null,
        metric: String(needsAttention),
        label: "blocked",
        action: "Review",
        onClick: onOpenWork,
      };
    }

    if (journalDailyPath) {
      return {
        kind: "daily" as const,
        title: journalDailyHeroTitle(
          journalDailyPath,
          vault.notes,
          vault.labelByPath(),
        ),
        metric: null as string | null,
        label: "journal daily",
        action: "Open daily",
        onClick: () => onOpenNote(journalDailyPath),
      };
    }

    return {
      kind: "chat" as const,
      title: null as string | null,
      metric: null as string | null,
      label: "ready",
      action: "Chat",
      onClick: onOpenChat,
    };
  });

  const motionColumns = $derived(
    KANBAN_COLUMNS.filter((column) => column !== "done" && column !== "blocked"),
  );

  const nextSchedule = $derived(recurring.soonestEnabled());
</script>

<section class="flex flex-1 flex-col items-center justify-center px-6 py-8">
  <div class="w-full max-w-md">
    <p class="workshop-faint uppercase tracking-widest">Briefing</p>

    {#if nextAction.metric}
      <p class="workshop-display mt-1">{nextAction.metric}</p>
      <p class="mt-1 text-lg text-surface-200">{nextAction.label}</p>
    {:else if nextAction.title}
      <h2 class="mt-1 text-2xl font-semibold leading-tight tracking-tight text-surface-50">
        {nextAction.title}
      </h2>
      <p class="mt-1 text-sm text-surface-400">{nextAction.label}</p>
    {:else}
      <p class="workshop-display mt-1 text-3xl">—</p>
      <p class="mt-1 text-lg text-surface-300">Start a conversation</p>
    {/if}

    <div class="mt-5 flex items-center gap-4">
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        onclick={nextAction.onClick}
      >
        {nextAction.action}
      </button>
      <button type="button" class="workshop-text-action" onclick={onOpenWork}>
        Work board →
      </button>
    </div>

    {#if nextSchedule}
      <p class="workshop-faint mt-4">
        Next · {recurring.labelFor(nextSchedule)} at
        {recurring.formatNextRun(nextSchedule.next_run_at_utc)}
      </p>
    {/if}

    {#if nextAction.kind !== "blocked"}
      <div
        class="mt-8 grid grid-cols-3 gap-px overflow-hidden rounded-md border border-surface-500/40 bg-surface-500/40"
      >
        {#each motionColumns as column (column)}
          <div class="bg-surface-800/90 px-3 py-2.5">
            <p class="workshop-faint capitalize">{columnLabel(column)}</p>
            <p class="mt-0.5 text-lg font-semibold tabular-nums text-surface-100">
              {workspace.columnCounts[column] ??
                workspace.cards.filter((card) => card.column === column).length}
            </p>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>
