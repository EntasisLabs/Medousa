<script lang="ts">
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { ProvenanceChip } from "$lib/utils/workHub";
  import { partitionWorkHub } from "$lib/utils/workHub";

  interface Props {
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
  }

  let { onSelectCard, onOpenNote, onOpenChat }: Props = $props();

  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);

  async function handleProvenance(chip: ProvenanceChip, cardId: string) {
    if (chip.kind === "vault" && chip.href) {
      onOpenNote(chip.href);
      return;
    }
    if (chip.kind === "chat") {
      const detail = workspace.cardDetailsCache.get(cardId);
      const sessionId = detail?.session_id?.trim();
      if (sessionId && sessionId !== chat.sessionId) {
        await chat.switchSession(sessionId);
      }
      onOpenChat();
      return;
    }
    void onSelectCard(cardId);
  }
</script>

<section class="work-hub-stage">
  <header class="work-hub-stage-header">
    <div class="flex min-w-0 items-start gap-2">
      <ShellSidebarExpandButton label="Show rail" />
      <div class="min-w-0">
        <h1 class="text-sm font-semibold text-surface-50">Work</h1>
        <p class="mt-0.5 text-[11px] text-surface-500">
          {#if living.length === 1}
            1 in motion
          {:else if living.length > 1}
            {living.length} in motion
          {:else}
            Nothing in motion
          {/if}
        </p>
      </div>
    </div>
  </header>

  <div class="work-hub-grid-wrap">
    {#if living.length === 0}
      <div class="work-hub-empty">
        <EmptyState
          title="Nothing in motion"
          description="Describe an ask below — manifestations land here while Medousa works."
        />
      </div>
    {:else}
      <div class="work-hub-grid">
        {#each living as card (card.id)}
          <WorkManifestCard
            {card}
            detail={workspace.cardDetailsCache.get(card.id)}
            selected={workspace.selectedCardId === card.id}
            onSelect={(id) => void onSelectCard(id)}
            onProvenance={handleProvenance}
          />
        {/each}
      </div>
    {/if}
  </div>
</section>
