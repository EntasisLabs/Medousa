<script lang="ts">
  import { onMount } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import KanbanBoard from "$lib/components/work/KanbanBoard.svelte";
  import CardInspector from "$lib/components/work/CardInspector.svelte";
  import AskCompletionModal from "$lib/components/work/AskCompletionModal.svelte";
  import NewWorkAsk from "$lib/components/work/NewWorkAsk.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";

  interface Props {
    visible: boolean;
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenNote, onOpenChat, onSelectCard }: Props = $props();

  const desktopRails = $derived(
    layoutDesktopRails({
      viewportWidth: layout.viewportWidth,
      activityCollapsed: layout.activityCollapsed,
      activityWidth: layout.activityWidth,
      workInspectorOpen: workspace.selectedCardId !== null,
      workInspectorWidth: layout.workInspectorWidth,
    }),
  );

  onMount(() => {
    void workspace.prefetchCardDetails();
  });
</script>

<div class="flex h-full min-h-0 min-w-0 flex-1 {visible ? '' : 'hidden'}">
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
    <KanbanBoard onSelectCard={onSelectCard} />
    <NewWorkAsk {visible} />
  </div>

  {#if workspace.selectedCardId}
    <SplitPane
      width={desktopRails.inspectorPaneWidth}
      side="right"
      min={280}
      max={desktopRails.inspectorPaneMax}
      onResize={(width) => layout.setWorkInspectorWidth(width)}
    >
      <CardInspector
        split={true}
        {onOpenNote}
        {onOpenChat}
        onClose={() => workspace.clearSelection()}
      />
    </SplitPane>
  {/if}

  <AskCompletionModal
    pending={workspace.pendingAskCompletion}
    {onOpenNote}
    onClose={() => workspace.clearPendingAskCompletion()}
  />
</div>
