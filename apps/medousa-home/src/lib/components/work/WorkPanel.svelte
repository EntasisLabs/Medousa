<script lang="ts">
  import { onMount } from "svelte";
  import WorkHub from "$lib/components/work/WorkHub.svelte";
  import WorkAsksPanel from "$lib/components/work/WorkAsksPanel.svelte";
  import WorkManifestPopover from "$lib/components/work/WorkManifestPopover.svelte";
  import AskCompletionModal from "$lib/components/work/AskCompletionModal.svelte";
  import UndertakingsPanel from "$lib/components/work/UndertakingsPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";

  interface Props {
    visible: boolean;
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenNote, onOpenChat, onSelectCard }: Props = $props();

  const showAsks = $derived(workspace.workView === "asks");
  const tab = $derived(undertakings.workTab);

  onMount(() => {
    void workspace.prefetchCardDetails();
  });
</script>

<div class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <div class="flex shrink-0 gap-1 border-b border-surface-500/40 px-3 pt-2">
    <button
      type="button"
      class="rounded-t px-3 py-1.5 text-xs {tab === 'activity'
        ? 'bg-surface-800 text-surface-50'
        : 'text-surface-400 hover:text-surface-200'}"
      onclick={() => undertakings.setWorkTab("activity")}
    >
      Activity
    </button>
    <button
      type="button"
      class="rounded-t px-3 py-1.5 text-xs {tab === 'undertakings'
        ? 'bg-surface-800 text-surface-50'
        : 'text-surface-400 hover:text-surface-200'}"
      onclick={() => undertakings.setWorkTab("undertakings")}
    >
      Undertakings
    </button>
  </div>

  {#if tab === "undertakings"}
    <UndertakingsPanel />
  {:else if showAsks}
    <WorkAsksPanel {onOpenChat} />
  {:else}
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <WorkHub {onSelectCard} {onOpenNote} {onOpenChat} />
    </div>
  {/if}

  <WorkManifestPopover {onOpenNote} {onOpenChat} />

  <AskCompletionModal
    pending={workspace.pendingAskCompletion}
    {onOpenNote}
    onClose={() => workspace.clearPendingAskCompletion()}
  />
</div>
