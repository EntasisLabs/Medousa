<script lang="ts">
  import { onMount } from "svelte";
  import KanbanBoard from "$lib/components/work/KanbanBoard.svelte";
  import CardInspector from "$lib/components/work/CardInspector.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    visible: boolean;
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenNote, onOpenChat, onSelectCard }: Props = $props();

  onMount(() => {
    void workspace.prefetchCardDetails();
  });
</script>

<div class="flex h-full min-w-0 flex-1 {visible ? '' : 'hidden'}">
  {#if workspace.workView === "inspector" && workspace.selectedCardId}
    <CardInspector
      {onOpenNote}
      {onOpenChat}
      onBack={() => workspace.showKanban()}
    />
  {:else}
    <KanbanBoard onSelectCard={onSelectCard} />
  {/if}
</div>
