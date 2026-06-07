<script lang="ts">
  import { recurring } from "$lib/stores/recurring.svelte";
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
        body: "Pick up where the workshop left off.",
        action: "Continue",
        onClick: () => onSelectCard(primaryCard.id),
      };
    }

    if (needsAttention > 0) {
      return {
        kind: "blocked" as const,
        title: `${needsAttention} blocked`,
        subtitle: "Needs attention",
        body: "Review stuck jobs before starting something new.",
        action: "Review work",
        onClick: onOpenWork,
      };
    }

    if (vault.selectedPath) {
      const title =
        vault.labelByPath().get(vault.selectedPath) ??
        vaultDisplayTitle(vault.title, vault.selectedPath);
      return {
        kind: "note" as const,
        title,
        subtitle: "Last note",
        body: "Jump back into your notes.",
        action: "Open note",
        onClick: () => onOpenNote(vault.selectedPath!),
      };
    }

    return {
      kind: "chat" as const,
      title: "Start a conversation",
      subtitle: "Get started",
      body: "What do you want to work on?",
      action: "Start chatting",
      onClick: onOpenChat,
    };
  });

  const heroTone = $derived(
    nextAction.kind === "blocked"
      ? "border-warning-500/30"
      : "border-primary-500/25",
  );

  const nextSchedule = $derived(recurring.soonestEnabled());
</script>

<section class="flex flex-1 flex-col items-center justify-center p-8">
  <div class="w-full max-w-xl">
    <div
      class="workshop-inset p-6 {heroTone}"
    >
      <div
        class="mb-4 h-0.5 w-12 rounded-full {nextAction.kind === 'blocked'
          ? 'bg-warning-500'
          : 'bg-primary-500'}"
      ></div>
      <p class="text-xs text-surface-300">{nextAction.subtitle}</p>
      <h2 class="mt-1 text-xl font-semibold text-surface-100">
        {nextAction.title}
      </h2>
      <p class="mt-3 text-sm text-surface-200">{nextAction.body}</p>
      {#if nextSchedule}
        <p class="workshop-faint mt-3">
          Next scheduled · {recurring.labelFor(nextSchedule)} at
          {recurring.formatNextRun(nextSchedule.next_run_at_utc)}
        </p>
      {/if}
      <button
        type="button"
        class="btn mt-5 {nextAction.kind === 'blocked'
          ? 'variant-filled-warning'
          : 'variant-filled-primary'}"
        onclick={nextAction.onClick}
      >
        {nextAction.action}
      </button>
    </div>

    {#if nextAction.kind !== "blocked"}
      <div class="mt-6 grid grid-cols-3 gap-3">
        {#each KANBAN_COLUMNS.filter((column) => column !== "done" && column !== "blocked") as column (column)}
          <div class="workshop-inset p-3">
            <p class="workshop-label capitalize">{columnLabel(column)}</p>
            <p class="mt-1 text-xl font-semibold text-surface-100">
              {workspace.columnCounts[column] ??
                workspace.cards.filter((card) => card.column === column).length}
            </p>
          </div>
        {/each}
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
