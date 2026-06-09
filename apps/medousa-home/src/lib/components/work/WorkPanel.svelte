<script lang="ts">
  import { onMount } from "svelte";
  import WorkHub from "$lib/components/work/WorkHub.svelte";
  import WorkManifestPopover from "$lib/components/work/WorkManifestPopover.svelte";
  import AskCompletionModal from "$lib/components/work/AskCompletionModal.svelte";
  import NewWorkAsk from "$lib/components/work/NewWorkAsk.svelte";
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

<div class="relative flex h-full min-h-0 min-w-0 flex-1 {visible ? '' : 'hidden'}">
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
    <WorkHub {onSelectCard} {onOpenNote} {onOpenChat} />
    <NewWorkAsk {visible} />
  </div>

  <WorkManifestPopover {onOpenNote} {onOpenChat} />

  <AskCompletionModal
    pending={workspace.pendingAskCompletion}
    {onOpenNote}
    onClose={() => workspace.clearPendingAskCompletion()}
  />
</div>
